// The glass body plus its sibling layers (edge highlight, tint).
//
// Inputs: the group's field texture, the backdrop composite, the group's
// capture (a full mip chain at the members' shared capture scale) and one
// recipe per member. Output: the backdrop with the composited glass.
//
// Colour math runs on premultiplied sRGB-encoded values (the specification's
// compositing space), unpremultiplied around the colour operations.

struct Recipe {
    shifts: vec4<f32>,        // primary shift, primary depth, rim shift, rim depth
    rim: vec4<f32>,           // rim ramp r0, r1, rim mix, blur radius
    blur_depths: vec4<f32>,   // d0..d3
    blur_gains: vec4<f32>,    // g0..g3
    haze: vec4<f32>,          // radius, min weight, max weight, mix
    limit_grade: vec4<f32>,   // luminance limit, grade mix, -, -
    grade: vec4<f32>,         // floor, ceiling, chroma, -
    wash: vec4<f32>,          // straight rgb, wash alpha
    ambient: vec4<f32>,       // shift, depth, blur radius, strength
    ambient_misc: vec4<f32>,  // ramp e0, e1, favours light, -
    ambient_grade: vec4<f32>, // floor, ceiling, chroma, -
    contour: vec4<f32>,       // density, shift, softness, width
    contour_misc: vec4<f32>,  // clip, -, -, -
    border_a: vec4<f32>,      // strength, bearing, band width, band offset
    border_b: vec4<f32>,      // aperture, colour bias, -, -
    cast_a: vec4<f32>,        // shift x, y, falloff, density
    cast_b: vec4<f32>,        // gain, exterior span, channel ceiling, -
    hl_a: vec4<f32>,          // inner fade, first gain, second gain, aperture
    hl_b: vec4<f32>,          // first bearing, second bearing, knee, -
    hl_bb: vec4<f32>,         // glow gain, width, aperture, has tint
    hl_first: vec4<f32>,      // straight rgba
    hl_second: vec4<f32>,     // straight rgba
    tint: vec4<f32>,          // straight rgb, effective alpha
}

struct MaterialUniforms {
    // scene width pt, scene height pt, px per pt, capture max lod
    scene: vec4<f32>,
    // display peak, -, -, -
    misc: vec4<f32>,
    // region min pt, region size pt: the pass runs on a patch covering the
    // group's bounds plus reach, and the capture covers the same rect
    region: vec4<f32>,
    // region origin px (field texel offset), -, -
    origin: vec4<f32>,
}

@group(0) @binding(0) var<uniform> uni: MaterialUniforms;
@group(0) @binding(1) var<storage, read> recipes: array<Recipe>;
@group(0) @binding(2) var field_tex: texture_2d<f32>;
@group(0) @binding(3) var backdrop_tex: texture_2d<f32>;
@group(0) @binding(4) var capture_tex: texture_2d<f32>;
@group(0) @binding(5) var linear_sampler: sampler;

const EPS_ALPHA: f32 = 1e-4;
const HIGHLIGHT_BAND: f32 = 1.33;

const LUMA_LIMIT_W: vec3<f32> = vec3<f32>(0.2126, 0.7153, 0.0722);
const LUMA_GATE_W: vec3<f32> = vec3<f32>(0.2125, 0.7153, 0.0721);

// ---------------------------------------------------------------- field ---

struct FieldSample {
    d: f32,
    n: vec2<f32>,
    owner: u32,
}

fn load_field(px: vec2<f32>) -> FieldSample {
    let dims = vec2<i32>(textureDimensions(field_tex));
    // `px` are absolute composite px; the field texture is region-local.
    let c = clamp(
        vec2<i32>(floor(px)) - vec2<i32>(uni.origin.xy),
        vec2<i32>(0),
        dims - vec2<i32>(1),
    );
    let v = textureLoad(field_tex, c, 0);
    var s: FieldSample;
    s.d = v.x;
    s.n = v.yz;
    s.owner = u32(max(v.w, 0.0));
    return s;
}

// ------------------------------------------------------------- sampling ---

// §3: the mip level the ambient sample reads for a radius of `b` capture
// texels.
fn level_of_radius(b: f32) -> f32 {
    let x = select(b, 1.0 + b * 0.5, b < 2.0);
    return max(log2(x), 0.0);
}

// The §3 blur of `sigma` level-0 capture texels (blur.wgsl) read on
// pyramid level `level` around `uv`.
fn gaussian_at_level(uv: vec2<f32>, sigma: f32, level: f32) -> vec4<f32> {
    let s = blur_deviation(sigma, level);
    let size = vec2<f32>(textureDimensions(capture_tex, i32(level)));
    // Texel centres sit at integers on this grid.
    let p = uv * size - 0.5;
    let base = floor(p);
    let x = blur_axis(p.x - base.x, s);
    let y = blur_axis(p.y - base.y, s);
    var acc = vec4<f32>(0.0);
    acc = blur_row(acc, base, x, y.offset.x, y.weight.x, size, level);
    acc = blur_row(acc, base, x, y.offset.y, y.weight.y, size, level);
    acc = blur_row(acc, base, x, y.offset.z, y.weight.z, size, level);
    acc = blur_row(acc, base, x, y.offset.w, y.weight.w, size, level);
    var total = 0.0;
    total = blur_row_total(total, x, y.weight.x);
    total = blur_row_total(total, x, y.weight.y);
    total = blur_row_total(total, x, y.weight.z);
    total = blur_row_total(total, x, y.weight.w);
    return acc / total;
}

// One row of the folded grid at row offset `oy` and weight `wy`, added
// to `acc` in the order the reads accumulate.
fn blur_row(
    acc: vec4<f32>,
    base: vec2<f32>,
    x: BlurAxis,
    oy: f32,
    wy: f32,
    size: vec2<f32>,
    level: f32,
) -> vec4<f32> {
    var a = acc;
    a += (x.weight.x * wy) * blur_read((base + vec2<f32>(x.offset.x, oy) + 0.5) / size, level);
    a += (x.weight.y * wy) * blur_read((base + vec2<f32>(x.offset.y, oy) + 0.5) / size, level);
    a += (x.weight.z * wy) * blur_read((base + vec2<f32>(x.offset.z, oy) + 0.5) / size, level);
    a += (x.weight.w * wy) * blur_read((base + vec2<f32>(x.offset.w, oy) + 0.5) / size, level);
    return a;
}

fn blur_read(at: vec2<f32>, level: f32) -> vec4<f32> {
    return textureSampleLevel(capture_tex, linear_sampler, at, level);
}

// §4.1/§4.3/§4.4: the shared capture at `q_pt`, blurred by a Gaussian of
// `sigma` capture texels. The reads run straight through, like the
// engine's `blurred_tap`.
fn sample_blurred(q_pt: vec2<f32>, sigma: f32) -> vec4<f32> {
    let uv = (q_pt - uni.region.xy) / uni.region.zw;
    let t = blur_position(sigma, uni.scene.w);
    let level = floor(t);
    return mix(
        gaussian_at_level(uv, sigma, level),
        gaussian_at_level(uv, sigma, min(level + 1.0, uni.scene.w)),
        t - level,
    );
}

// §4.1: the shared capture sampled at `level(b)`, never below 0.
fn sample_capture(q_pt: vec2<f32>, b: f32) -> vec4<f32> {
    let uv = (q_pt - uni.region.xy) / uni.region.zw;
    let lod = clamp(level_of_radius(b), 0.0, uni.scene.w);
    return textureSampleLevel(capture_tex, linear_sampler, uv, lod);
}

fn sample_backdrop(q_pt: vec2<f32>) -> vec4<f32> {
    let uv = q_pt / uni.scene.xy;
    return textureSampleLevel(backdrop_tex, linear_sampler, uv, 0.0);
}

// §3: the quarter-circle lobe, `shift · (1 − sqrt(t·(2 − t)))`,
// `t = sat(−d / depth)`.
fn lobe_at(d: f32, depth: f32, shift: f32) -> f32 {
    return shift * meniscus(-d, depth);
}

// §4.3: the piecewise blur gain through (d_i, g_i), held at the ends;
// equal consecutive depths step.
fn blur_gain(r: Recipe, d: f32) -> f32 {
    let xs = array<f32, 4>(r.blur_depths.x, r.blur_depths.y, r.blur_depths.z, r.blur_depths.w);
    let ys = array<f32, 4>(r.blur_gains.x, r.blur_gains.y, r.blur_gains.z, r.blur_gains.w);
    if d <= xs[0] {
        return ys[0];
    }
    for (var i = 0; i < 3; i += 1) {
        if d < xs[i + 1] {
            let span = xs[i + 1] - xs[i];
            if span <= 1e-6 {
                return ys[i + 1];
            }
            let t = (d - xs[i]) / span;
            return ys[i] + t * (ys[i + 1] - ys[i]);
        }
    }
    return ys[3];
}

// §4.3: the sample's blur radius — the recipe's radius times the gain at
// the refracted depth.
fn blur_radius_at(r: Recipe, d: f32) -> f32 {
    return r.rim.w * blur_gain(r, d);
}

// --------------------------------------------------------------- colour ---

fn unpremultiply(c: vec4<f32>) -> vec3<f32> {
    let a = max(c.a, EPS_ALPHA);
    var s = c.rgb / a;
    s = select(s, vec3<f32>(0.0), abs(s) < vec3<f32>(EPS_ALPHA));
    return s;
}

// §4.6's grade: the BT.709 YCbCr construction — luma remapped onto
// [floor, ceiling], both chroma channels re-scaled about 0.5 by chroma.
fn apply_grade(grade: vec3<f32>, c: vec3<f32>) -> vec3<f32> {
    let y = luma(c);
    let cb = (c.b - y) * (1.0 / 1.8556) + 0.5;
    let cr = (c.r - y) * (1.0 / 1.5748) + 0.5;
    let y2 = grade.x + y * (grade.y - grade.x);
    let cb2 = 0.5 + grade.z * (cb - 0.5);
    let cr2 = 0.5 + grade.z * (cr - 0.5);
    let r = y2 + 1.5748 * (cr2 - 0.5);
    let b = y2 + 1.8556 * (cb2 - 0.5);
    let g = (y2 - 0.2126 * r - 0.0722 * b) * (1.0 / 0.7152);
    return vec3<f32>(r, g, b);
}

fn ease_cubic(t: f32) -> f32 {
    // Cubic bezier with control points (0.61, 0.007) and (0.47, 0.99);
    // x -> y by bisection.
    let p1 = vec2<f32>(0.60938, 0.00663);
    let p2 = vec2<f32>(0.47124, 0.99115);
    var lo = 0.0;
    var hi = 1.0;
    for (var i = 0; i < 12; i += 1) {
        let mid = 0.5 * (lo + hi);
        let u = 1.0 - mid;
        let x = 3.0 * u * u * mid * p1.x + 3.0 * u * mid * mid * p2.x + mid * mid * mid;
        if x < t {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let s = 0.5 * (lo + hi);
    let u = 1.0 - s;
    return 3.0 * u * u * s * p1.y + 3.0 * u * s * s * p2.y + s * s * s;
}

// ------------------------------------------------------------- material ---

// The §4.2–§4.9 glass body: the straight (unpremultiplied) colour inside
// the silhouette.
fn glass_body(
    r: Recipe,
    p: vec2<f32>,
    d: f32,
    n: vec2<f32>,
    cov: f32,
    px_per_pt: f32,
) -> vec3<f32> {
    // §4.2: the primary sample, refracted by the primary lobe and blurred at
    // its refracted depth.
    let o1 = lobe_at(d, r.shifts.y, r.shifts.x);
    var C = unpremultiply(sample_blurred(
        p + n * o1,
        BLUR_SIGMA_PER_RADIUS * blur_radius_at(r, d + o1),
    ));

    // §4.4: the haze — the capture at the pixel blurred by a Gaussian of
    // the haze radius's deviation, unpremultiplied; runs before the
    // rim-sample mix.
    let radius = r.haze.x;
    if radius > 0.0 {
        let F = unpremultiply(sample_blurred(p, HAZE_SIGMA_PER_RADIUS * radius));
        let c1 = r.haze.y * min(C, F)
            + r.haze.z * max(C, F)
            + (1.0 - r.haze.y - r.haze.z) * C;
        C = mix(c1, F, r.haze.w);
    }

    // §4.2: the rim sample and the rim-ramp mix; a rim mix of 0 takes no
    // sample.
    if r.rim.z > 0.0 {
        let o2 = lobe_at(d, r.shifts.w, r.shifts.z);
        let s_rim = unpremultiply(sample_blurred(
            p + n * o2,
            BLUR_SIGMA_PER_RADIUS * blur_radius_at(r, d + o2),
        ));
        let tw = sat((d - r.rim.x) / (r.rim.y - r.rim.x));
        C = mix(C, s_rim, r.rim.z * tw);
    }

    // §4.5: the luminance limit [fitted].
    let lum = dot(LUMA_LIMIT_W, C);
    let a = sat(1.0 - lum * (1.0 - r.limit_grade.x));
    C = mix(vec3<f32>(a * lum), a * C, 1.0 + (1.0 - a) * 0.3);

    // §4.6: the grade — matrix, wash, composite by the grade mix.
    let graded = apply_grade(r.grade.xyz, C);
    C = mix(
        C,
        (1.0 - r.wash.a) * graded + r.wash.a * r.wash.rgb,
        r.limit_grade.y,
    );

    // §4.7: the ambient pickup — a sample pushed out past the contour,
    // graded, gated by the current luma; not clamped.
    let o_b = lobe_at(d, r.ambient.y, r.ambient.x);
    let ambient_sample = apply_grade(
        r.ambient_grade.xyz,
        unpremultiply(sample_capture(p + n * o_b, r.ambient.z)),
    );
    let gate = dot(LUMA_GATE_W, C);
    let k = select(1.0 - gate, gate, r.ambient_misc.z > 0.5);
    let band = sat((d - r.ambient_misc.x) / (r.ambient_misc.y - r.ambient_misc.x));
    let g = band * k * k;
    let w = r.ambient.w * g * g;
    C = mix(C, ambient_sample, w);

    // §4.8: the contour shade — the band of the shifted field's falloff
    // profile lying just inside the shifted contour, limited to the glass
    // coverage by the clip. The shape weight `wR` is taken as 1 for every
    // member [chosen].
    if r.contour.x > 0.0 {
        let f_r = load_field((p - vec2<f32>(0.0, r.contour.y)) * px_per_pt);
        let alpha_r = r.contour.x
            * sat(
                shadow_profile(f_r.d / r.contour.z)
                    - shadow_profile((f_r.d + r.contour.w) / r.contour.z),
            )
            * mix(1.0, cov, r.contour_misc.x);
        C = C * (1.0 - alpha_r);
    }

    // §4.9: the border correction — two opposing edge lobes around the rim.
    if r.border_a.x > 0.0 {
        let u = vec2<f32>(sin(r.border_a.y), -cos(r.border_a.y));
        let cs = cos(0.5 * r.border_b.x);
        let band_b = sat(1.0 - abs(d - r.border_a.w) / r.border_a.z);
        let q1 = sat((dot(n, u) - cs) / (1.0 - cs));
        let q2 = sat((dot(n, -u) - cs) / (1.0 - cs));
        let h = band_b * (q1 / max(1.0 + r.border_a.x * (1.0 - q1), 1e-4)
            + q2 / max(1.0 + r.border_a.x * (1.0 - q2), 1e-4));
        C = C * (1.0 + r.border_b.y * h * (3.0 - 2.0 * C));
    }

    return C;
}

// §3's falloff profile applied to the shadow's normalized distance.
fn shadow_profile(q: f32) -> f32 {
    return falloff(4.0 * sat(0.17676 * q + 0.5) - 2.0);
}

// The §4.12 two-light rim highlight plus the glow term — a second band of
// the same form at the glow multipliers [chosen: §4.12 leaves the term's
// form open].
fn highlight(r: Recipe, outc: vec3<f32>, d: f32, n: vec2<f32>, cov: f32) -> vec3<f32> {
    if d >= 0.5 {
        return outc;
    }
    let aperture = r.hl_a.w;
    let cs = cos(0.5 * aperture);
    let cs2 = cos(0.5 * aperture * r.hl_bb.z);
    let band_h = HIGHLIGHT_BAND;
    let band_h2 = HIGHLIGHT_BAND * r.hl_bb.y;
    var band = vec2<f32>(0.0);
    // narrow band
    let s = sat(-d / band_h);
    band.x = (1.0 - r.hl_a.x * s) * (1.0 - s) * cov;
    // glow band
    let s2 = sat(-d / band_h2);
    band.y = (1.0 - r.hl_a.x * s2) * (1.0 - s2) * cov;
    let bearings = vec2<f32>(r.hl_b.x, r.hl_b.y);
    let gains = vec2<f32>(r.hl_a.y, r.hl_a.z);
    var c_narrow = vec3<f32>(0.0);
    var c_broad = vec3<f32>(0.0);
    for (var i = 0; i < 2; i += 1) {
        let u = vec2<f32>(sin(bearings[i]), -cos(bearings[i]));
        let q = sat((dot(n, u) - cs) / (1.0 - cs));
        let lobe = q / (1.0 + r.hl_b.z * (1.0 - q));
        let q2 = sat((dot(n, u) - cs2) / (1.0 - cs2));
        let lobe2 = q2 / (1.0 + r.hl_b.z * (1.0 - q2));
        let colour = select(r.hl_second, r.hl_first, i == 0);
        c_narrow += colour.rgb * colour.a * gains[i] * 2.0 * lobe;
        c_broad += colour.rgb * colour.a * gains[i] * r.hl_bb.x * 2.0 * lobe2;
    }
    // The highlight adds to the accumulated colour under it; its gain
    // follows that colour's luma [chosen: keeps the current form].
    let gain = 0.2 + 0.8 * luma(outc);
    return min(outc + (c_narrow * band.x + c_broad * band.y) * gain, vec3<f32>(1.0));
}

// §4.13: the tint layers — a fill of the shape, then an edge gradient at
// full strength from −1 to 0 pt that eases off to zero 10 pt inward. The
// measured stops (−1, 0, 10 pt) are read as distances measured inward
// [chosen]. The tint's alpha is the effective one (§4.13's
// composite strength is the recipe's, `Tint::declared`), and the gradient's gain follows the luma
// beneath it [chosen: keeps the current form].
fn tint_layers(r: Recipe, outc: vec3<f32>, d: f32, cov: f32) -> vec3<f32> {
    if r.hl_bb.w < 0.5 || cov <= 0.0 {
        return outc;
    }
    let a_flat = r.tint.a * cov;
    var c = r.tint.rgb * a_flat + outc * (1.0 - a_flat);
    // A flat tint (flag 2, §8.1) takes no edge gradient.
    let edge = step(r.hl_bb.w, 1.5);
    let t = sat(-d / 10.0);
    let a_rim = edge * r.tint.a * (1.0 - ease_cubic(t)) * cov;
    let gain = 0.2 + 0.8 * luma(c);
    c = r.tint.rgb * gain * a_rim + c * (1.0 - a_rim);
    return c;
}

// ---------------------------------------------------------------- entry ---

@fragment
fn material_fragment(v: FullscreenOut) -> @location(0) vec4<f32> {
    // The pass draws on a region-sized patch: local px + origin = scene px.
    let px = v.position.xy + uni.origin.xy;
    let p = px / uni.scene.z;
    let f = load_field(px);
    let r = recipes[min(f.owner, arrayLength(&recipes) - 1u)];
    let fw = fwidth(f.d);
    let cov = sat(0.5 - f.d / fw);
    let backdrop = sample_backdrop(p);

    // §4.10: the cast shadow — plain black, no backdrop sample, at the
    // shifted field.
    let f_s = load_field((p - r.cast_a.xy) * uni.scene.z);
    var cast_alpha = 0.0;
    if r.cast_a.w > 0.0 {
        cast_alpha = r.cast_a.w * r.cast_b.x
            * falloff(4.0 * sat(0.25 * f_s.d / r.cast_a.z + 0.5) - 2.0);
    }

    // Past the element's exterior span the pass returns the backdrop
    // (§4.11); the shadow can still cover the pixel.
    if f.d > r.cast_b.y && cast_alpha < 1e-4 {
        return backdrop;
    }

    var outc = backdrop;
    if cov > 0.0 {
        let n = f.n / max(length(f.n), 1e-4);
        var C = glass_body(r, p, f.d, n, cov, uni.scene.z);
        // §4.11: the per-channel ceiling, then the display peak.
        C = clamp(C, vec3<f32>(0.0), vec3<f32>(min(r.cast_b.z, uni.misc.x)));
        // §4.12: the highlight sibling.
        // §4.13: the tint siblings; a flat tint (§8.1) lies under the
        // highlight, every other tint over it.
        C = tint_layers(r, highlight(r, C, f.d, n, cov), f.d, cov);
        let interior = vec4<f32>(C, 1.0);
        let shadow = vec4<f32>(0.0, 0.0, 0.0, cast_alpha);
        let res = mix(shadow, interior, cov);
        outc = res + backdrop * (1.0 - res.a);
    } else {
        // Outside the silhouette: the cast shadow over the backdrop.
        outc = backdrop * (1.0 - cast_alpha);
    }
    return outc;
}
