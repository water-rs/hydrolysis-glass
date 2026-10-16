// §3: the blur a radius of `b` makes — the capture convolved with a
// Gaussian of standard deviation `BLUR_SIGMA_PT_PER_RADIUS · b` pt
// [fitted], whatever the capture scale; each shader carries it onto its
// capture's texels. Shared by the standalone and the engine material
// shaders, each of which reads its own capture.
//
// It is realized on the capture's 2×2 box pyramid [chosen]: on a level
// whose texel leaves at most `BLUR_LEVEL_SIGMA` texels of deviation, a
// (2·BLUR_TAPS + 1)² grid of bilinear taps one texel apart, weighted for
// the deviation the level's box reduction has not already made, blended
// with the next level by the fractional level so the blur is continuous
// in `b`. At `b = 0` it is the level-0 bilinear read.
//
// The tap grid is read folded: a bilinear tap at a fractional texel
// position is a fixed weighting of the two texels around it per axis, so
// the grid's 2·BLUR_TAPS + 1 taps weight 2·BLUR_TAPS + 2 texels per axis,
// and each adjacent pair of those is one bilinear read at the position
// that splits the pair's weight. The folded grid is BLUR_PAIRS² reads
// and the same sum.
const BLUR_SIGMA_PT_PER_RADIUS: f32 = 1.6666666;
// §4.4: the haze's Gaussian deviation, pt, per unit of haze radius
// [fitted].
const HAZE_SIGMA_PT_PER_RADIUS: f32 = 2.0;
const BLUR_TAPS: i32 = 3;
const BLUR_LEVEL_SIGMA: f32 = 1.5;
const BLUR_PAIRS: i32 = BLUR_TAPS + 1;
// The least deviation the tap grid is weighted for. At it every tap but
// the centre one has no weight, so the folded grid is the bilinear read
// at the sample point, and a sharper read takes the blur's own path.
const BLUR_MIN_DEVIATION: f32 = 1e-4;
// The least pair weight a split is divided by, the smallest normal f32.
// At a small deviation the outer pairs weigh nothing or a subnormal,
// which GPUs that divide as `hi · rcp(w)` and flush subnormals turn into
// an inf or NaN split; a pair that light takes no share of the read
// either way.
const BLUR_MIN_WEIGHT: f32 = 0x1p-126f;

// The pyramid position, a level and its fraction, the blur of `sigma`
// level-0 texels reads, on a pyramid whose top level is `top`.
fn blur_position(sigma: f32, top: f32) -> f32 {
    return clamp(log2(max(sigma, 1e-6) / BLUR_LEVEL_SIGMA), 0.0, top);
}

// The deviation, in texels of pyramid level `level`, the tap grid makes
// so the read is the Gaussian of `sigma` level-0 texels: a level-k texel
// is the mean of 2^k × 2^k level-0 texels, whose box reduction already
// has variance (4^k − 1)/12 level-0 texels² per axis. It is at least
// BLUR_MIN_DEVIATION.
fn blur_deviation(sigma: f32, level: f32) -> f32 {
    let scale = exp2(level);
    let reduced = (scale * scale - 1.0) / 12.0;
    return max(sqrt(max(sigma * sigma - reduced, 0.0)) / scale, BLUR_MIN_DEVIATION);
}

// One axis of the folded tap grid: component `k` is pair `k`'s read
// offset in texels from the texel at or below the sample point, and its
// weight. The grid has BLUR_PAIRS = 4 pairs, one per component.
struct BlurAxis {
    offset: vec4<f32>,
    weight: vec4<f32>,
}

// The Gaussian weight of the tap `i` texels from the sample point.
fn blur_tap(i: i32, s: f32) -> f32 {
    return select(0.0, exp(-0.5 * f32(i * i) / (s * s)), abs(i) <= BLUR_TAPS);
}

// The weight texel `m` (counted from the texel at or below the sample
// point, which sits `phase` texels past it) takes from the tap grid of
// deviation `s`: tap `m` reads it with weight `1 − phase`, tap `m − 1`
// with `phase`.
fn blur_texel(m: i32, phase: f32, s: f32) -> f32 {
    return blur_tap(m, s) * (1.0 - phase) + blur_tap(m - 1, s) * phase;
}

// The folded axis of the tap grid of deviation `s` around a sample point
// `phase` texels past a texel centre: pair `k` covers texels
// `a = 2k − BLUR_TAPS` and `a + 1`.
fn blur_axis(phase: f32, s: f32) -> BlurAxis {
    let a = vec4<i32>(0, 2, 4, 6) - vec4<i32>(BLUR_TAPS);
    let lo = vec4<f32>(
        blur_texel(a.x, phase, s),
        blur_texel(a.y, phase, s),
        blur_texel(a.z, phase, s),
        blur_texel(a.w, phase, s),
    );
    let hi = vec4<f32>(
        blur_texel(a.x + 1, phase, s),
        blur_texel(a.y + 1, phase, s),
        blur_texel(a.z + 1, phase, s),
        blur_texel(a.w + 1, phase, s),
    );
    let w = lo + hi;
    return BlurAxis(vec4<f32>(a) + hi / max(w, vec4<f32>(BLUR_MIN_WEIGHT)), w);
}

// The running weight total of one row of the folded grid, the row of
// weight `wy`, added in the order the row's reads accumulate.
fn blur_row_total(total: f32, x: BlurAxis, wy: f32) -> f32 {
    var t = total;
    t += x.weight.x * wy;
    t += x.weight.y * wy;
    t += x.weight.z * wy;
    t += x.weight.w * wy;
    return t;
}
