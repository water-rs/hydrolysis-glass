// §3: the blur a radius of `b` capture texels makes — the capture
// convolved with a Gaussian of standard deviation
// `BLUR_SIGMA_PER_RADIUS · b` texels [fitted]. Shared by the standalone
// and the engine material shaders, each of which reads its own capture.
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
const BLUR_SIGMA_PER_RADIUS: f32 = 2.5;
const BLUR_TAPS: i32 = 3;
const BLUR_LEVEL_SIGMA: f32 = 1.5;
const BLUR_PAIRS: i32 = BLUR_TAPS + 1;
// The least pair weight a split is divided by: the smallest normal f32.
// GPUs may divide as `hi · rcp(w)` and flush subnormals, D3D among them,
// so a subnormal `w` would make the split inf or NaN, and a NaN read
// position poisons the sum even at zero weight. A pair that light takes
// no share of the read either way.
const BLUR_MIN_WEIGHT: f32 = 0x1p-126f;

// The pyramid position, a level and its fraction, the blur of `sigma`
// level-0 texels reads, on a pyramid whose top level is `top`.
fn blur_position(sigma: f32, top: f32) -> f32 {
    return clamp(log2(max(sigma, 1e-6) / BLUR_LEVEL_SIGMA), 0.0, top);
}

// The deviation, in texels of pyramid level `level`, the tap grid makes
// so the read is the Gaussian of `sigma` level-0 texels: a level-k texel
// is the mean of 2^k × 2^k level-0 texels, whose box reduction already
// has variance (4^k − 1)/12 level-0 texels² per axis.
fn blur_deviation(sigma: f32, level: f32) -> f32 {
    let scale = exp2(level);
    let reduced = (scale * scale - 1.0) / 12.0;
    return sqrt(max(sigma * sigma - reduced, 0.0)) / scale;
}

// One axis of the folded tap grid: for pair `k`, the read's offset in
// texels from the texel at or below the sample point, and its weight.
struct BlurAxis {
    offset: array<f32, BLUR_PAIRS>,
    weight: array<f32, BLUR_PAIRS>,
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
    var axis: BlurAxis;
    for (var k = 0; k < BLUR_PAIRS; k += 1) {
        let a = 2 * k - BLUR_TAPS;
        let lo = blur_texel(a, phase, s);
        let hi = blur_texel(a + 1, phase, s);
        let w = lo + hi;
        axis.offset[k] = f32(a) + hi / max(w, BLUR_MIN_WEIGHT);
        axis.weight[k] = w;
    }
    return axis;
}
