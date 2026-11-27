// shaders/material_engine.wgsl — the recipe's engine form (issue #2).
//
// `theme::materials` registers this source as a Cherenkov
// `BackdropShaderSource`; the backdrop-group pass invokes
// `backdrop_effect` once per member pixel. The contract —
// `BackdropPixel`, `backdrop_sample_level`, `backdrop_field`, the
// premultiplied return in the capture's colour space — is documented in
// cherenkov-record/src/backdrop.rs. The capture stores the canvas's
// space — premultiplied linear Display P3 — and the glass capture class
// declares `BlendSpace::Linear`, so averaging, blending and compositing all
// stay in native Display P3 without sRGB gamut clipping. Reads inside the
// material body apply the Display P3 (IEC 61966-2-1) transfer curve directly
// to `LinearDisplayP3` via `space_srgb_encode` (extended past 1.0 for HDR),
// and the return decodes directly back via `space_srgb_decode`.
//
// The body is the standalone `shaders/material.wgsl` retargeted: field
// reads are `px.sdf`/`backdrop_field`, every blurred read is
// `backdrop_sample_blur` on the group's Gaussian pyramid, and the ellipse
// bend the standalone baked into its field texture is applied to
// `px.normal` here. The recipe's 23-vec4 layout does not fit the
// contract's sixteen parameters, so the packing keeps every
// recipe-varying field and folds the recipe's constant block — the
// highlight's shared terms, the border correction's measured constants, and
// the fixed ramps/mix — into shader constants that `theme::materials::uniforms`
// asserts (see its packing table).
//
// Parameter layout (must match `theme::materials::uniforms`):
//   p0  shifts          primary_shift, primary_depth, rim_shift, rim_depth
//   p1  rim             touch_x, touch_y, rim mix, blur radius
//   p2  blur_depths     d0..d3
//   p3  blur_gains      g0..g3
//   p4  haze            radius, min weight, max weight, mix
//   p5  limit/misc      luminance limit, press progress, recipe capture scale, exterior span
//   p6  grade           floor, ceiling, chroma, wash alpha
//   p7  wash            wash r, g, b, ambient favours light
//   p8  ambient         shift, depth, blur radius, strength
//   p9  ambient misc    has_touch (0/1), rim_span, ambient grade floor, ambient grade ceiling
//   p10 ambient/contour ambient grade chroma, contour density, shift, softness
//   p11 contour/border  contour width, contour clip, border aperture, cast falloff
//   p12 cast/highlight  cast density, cast gain, channel ceiling, hl aperture
//   p13 tint            straight r, g, b, effective alpha
//   p14 cast/rect       cast shift x, cast shift y, member rect x, member rect y
//   p15 rect/bend       member rect w, member rect h, ellipse bend, tint flag (0 none, 1 gradient, 2 flat)
//
// Distances the recipe calls pt arrive through `px.sdf`/`px.scale`; the
// §3 and haze deviations are pt, carried onto the bound capture's grid
// by its texels per pt (`backdrop_scale · px.scale`). The ambient radius
// is a recipe capture-texel count, scaled onto the bound capture's grid
// by `backdrop_scale / p5.z` before `ambient_sigma` turns it into a
// deviation — a member's capture class snaps its recipe's requested
// scale up to the registered lattice (see `theme::materials`), so the
// compensation keeps every physical radius the recipe's. The member rect
// rides in member-local pt.

const PI: f32 = 3.141592653589793;
const EPS_ALPHA: f32 = 1e-4;
const HIGHLIGHT_BAND: f32 = 1.5;
const BLEED_RADIUS: f32 = 36.0;
const CONTAINER_UNION_SMOOTHING_PT: f32 = 30.0;
const DISPERSION_SHIFT_PT: f32 = 2.5;

// The recipe's constant block, asserted by `theme::materials::uniforms`:
// `Highlight::for_size` shares every term but the aperture, and the
// border correction's measured constants are one row.
const HL_INNER_FADE: f32 = 0.75;
const HL_FIRST_GAIN: f32 = 0.5;
const HL_SECOND_GAIN: f32 = 0.5;
const HL_FIRST_BEARING: f32 = 0.0;
const HL_SECOND_BEARING: f32 = PI;
const HL_KNEE: f32 = 0.5;
const HL_GLOW_GAIN: f32 = 0.15;
const HL_GLOW_WIDTH: f32 = 8.0;
const HL_GLOW_APERTURE: f32 = 0.65;
const HL_FIRST_COLOUR: vec4<f32> = vec4<f32>(1.0);
const HL_SECOND_COLOUR: vec4<f32> = vec4<f32>(1.0);

const BORDER_STRENGTH: f32 = 0.4;
const BORDER_BEARING: f32 = PI * 0.5;
const BORDER_BAND_WIDTH: f32 = 0.533;
const BORDER_BAND_OFFSET: f32 = 0.25;

// Display P3 luminance weights.
const LUMA_WEIGHTS: vec3<f32> = vec3<f32>(0.22897456, 0.69173852, 0.07928691);
const LUMA_LIMIT_W: vec3<f32> = vec3<f32>(0.22897456, 0.69173852, 0.07928691);
const LUMA_GATE_W: vec3<f32> = vec3<f32>(0.22897456, 0.69173852, 0.07928691);

// Named unlike the engine module's own `sat` (which takes a vec3): the
// user source is appended to the same module, so names may not collide.
fn gsat(x: f32) -> f32 {
    return clamp(x, 0.0, 1.0);
}

// The meniscus profile mu(x; h) = 1 - sqrt(t (2 - t)), t = gsat(x / h).
fn meniscus(x: f32, h: f32) -> f32 {
    if h <= 0.0 {
        return select(0.0, 1.0, x <= 0.0);
    }
    let t = gsat(x / h);
    return 1.0 - sqrt(t * (2.0 - t));
}

// §3: the shadow falloff profile — a smooth step running from 1 at
// x = -2 to 0 at x = 2.
fn falloff(x: f32) -> f32 {
    let x2 = x * x;
    return 0.5 + x * (-0.5605 + x2 * (0.16821 + x2 * (-0.034454 + x2 * 0.0029545)));
}

// §3's falloff profile applied to a normalized shadow distance.
fn shadow_profile(q: f32) -> f32 {
    return falloff(4.0 * gsat(0.17676 * q + 0.5) - 2.0);
}

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, LUMA_WEIGHTS);
}

// The recipe's unpremultiply: alpha floored at ε and sub-ε straight
// channels zeroed, exactly the standalone's form.
fn glass_unpremultiply(c: vec4<f32>) -> vec3<f32> {
    let a = max(c.a, EPS_ALPHA);
    var s = c.rgb / a;
    s = select(s, vec3<f32>(0.0), abs(s) < vec3<f32>(EPS_ALPHA));
    return s;
}

// A premultiplied linear Display P3 sample to the recipe's straight Display
// P3-encoded working space using the extended IEC 61966-2-1 transfer curve
// with no sRGB gamut clipping.
fn glass_work_space(c: vec4<f32>) -> vec3<f32> {
    return space_srgb_encode(glass_unpremultiply(c));
}

// §3: the quarter-circle lobe, `shift · meniscus(−d; depth)`.
fn lobe_at(d: f32, depth: f32, shift: f32) -> f32 {
    return shift * meniscus(-d, depth);
}

// §4.3: the piecewise blur gain through (d_i, g_i), held at the ends;
// equal consecutive depths step.
fn blur_gain(depths: vec4<f32>, gains: vec4<f32>, d: f32) -> f32 {
    let xs = array<f32, 4>(depths.x, depths.y, depths.z, depths.w);
    let ys = array<f32, 4>(gains.x, gains.y, gains.z, gains.w);
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
    let g = (y2 - 0.22897456 * r - 0.07928691 * b) * (1.0 / 0.69173852);
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

// The inscribed ellipse's normal at `pt`, like the standalone's field
// pass: `rect` is (x, y, w, h) in member-local pt.
fn ellipse_normal(rect: vec4<f32>, pt: vec2<f32>) -> vec2<f32> {
    let half = max(rect.zw * 0.5, vec2<f32>(1e-6));
    let c = rect.xy + half;
    let v = (pt - c) / (half * half);
    let len = length(v);
    if len < 1e-12 {
        return vec2<f32>(0.0, -1.0);
    }
    return v / len;
}

// Evaluates the container smooth-union SDF (device px) and outward unit normal
// over ONLY fusing (translucent, `emission.w >= 0.0`) members, so prominent
// capsules never form a geometric bridge with translucent glass siblings (§3.1).
fn fusing_union_field(q: vec2<f32>, count: u32, u: f32) -> vec3<f32> {
    let k = CONTAINER_UNION_SMOOTHING_PT * u;
    var fused = 0.0;
    var fused_g = vec2<f32>(0.0);
    var last_d = -3.4e38;
    var last_j = -1;
    var initialized = false;
    for (var step = 0u; step < count; step += 1u) {
        var next_d = 3.4e38;
        var next_g = vec2<f32>(0.0);
        var next_j = i32(count);
        for (var j = 0u; j < count; j += 1u) {
            let sib = backdrop_sibling(j, q);
            if sib.emission.w < 0.0 {
                continue;
            }
            let d = sib.sdf;
            let ji = i32(j);
            let after_last = d > last_d || (d == last_d && ji > last_j);
            let before_next = d < next_d || (d == next_d && ji < next_j);
            if after_last && before_next {
                next_d = d;
                next_g = sib.normal;
                next_j = ji;
            }
        }
        if next_j >= i32(count) {
            break;
        }
        if !initialized {
            fused = next_d;
            fused_g = next_g;
            initialized = true;
        } else {
            let h = max(k - abs(fused - next_d), 0.0) / k;
            fused = min(fused, next_d) - h * h * k * 0.25;
            fused_g = (1.0 - 0.5 * h) * fused_g + 0.5 * h * next_g;
        }
        last_d = next_d;
        last_j = next_j;
    }
    let w = max(length(fused_g), 1e-6);
    return vec3<f32>(fused, fused_g / w);
}

// Signed geometric distance (device px) at `q` with sibling geometry separated
// from prominent capsules (§3.1).
fn geom_sdf_at(
    q: vec2<f32>,
    sibling_count: u32,
    self_index: u32,
    is_prominent: bool,
    has_prominent_sib: bool,
    u: f32,
) -> f32 {
    if sibling_count > 1u {
        if is_prominent {
            return backdrop_sibling(self_index, q).sdf;
        }
        if has_prominent_sib {
            return fusing_union_field(q, sibling_count, u).x;
        }
    }
    return backdrop_field(q).sdf;
}

// The §4.2–§4.9 glass body: the straight encoded colour inside the
// silhouette. `p` is the member pixel in device space, `d` the field
// distance in pt, `n` the bent field normal, `u` device px per pt,
// `cov` the silhouette coverage.
fn glass_body(
    params: array<vec4<f32>, 16>,
    p: vec2<f32>,
    d: f32,
    n: vec2<f32>,
    u: f32,
    cov: f32,
    sibling_count: u32,
    self_index: u32,
    is_prominent: bool,
    has_prominent_sib: bool,
) -> vec3<f32> {
    // Recipe capture texels onto the bound capture's grid: the class's
    // scale may sit above the recipe's requested scale (classes are
    // registered, recipes are continuous — see `theme::materials`), so a
    // radius of `b` recipe texels is `b · backdrop_scale / recipe scale`
    // texels here, keeping the physical ambient radius in pt the
    // recipe's.
    let k_tex = backdrop_scale / params[5].z;
    // The bound capture's texels per pt, which the §3 and haze
    // deviations (pt) are read on.
    let tex_per_pt = backdrop_scale * u;

    // §1.2 / §4.2: the primary sample, refracted by the primary lobe and
    // blurred at its refracted depth; Clear lenses integrate 7 spectral
    // taps across grazing lips (`x < min(0.18 m, 8.0)`).
    let o1 = lobe_at(d, params[0].y, params[0].x);
    let p1 = p + n * (o1 * u);
    let sigma1 = BLUR_SIGMA_PT_PER_RADIUS
        * params[1].w
        * blur_gain(params[2], params[3], d + o1)
        * tex_per_pt;
    var c: vec3<f32>;
    let m_elem = min(params[15].x, params[15].y);
    let is_clear = params[12].y < 0.2;
    let h_disp = min(0.18 * m_elem, 8.0);
    let mu_disp = meniscus(-d, h_disp);
    if is_clear && mu_disp > 1e-4 {
        let v_disp = n * (DISPERSION_SHIFT_PT * mu_disp * u);
        var acc = vec3<f32>(0.0);
        for (var step = 1; step <= 3; step += 1) {
            let k = f32(step) / 3.0;
            let enc = glass_work_space(backdrop_sample_blur(p1 + v_disp * k, sigma1));
            acc.r += k * enc.r;
            acc.g += (1.0 - k) * enc.g;
        }
        for (var step = 0; step <= 3; step += 1) {
            let k = f32(step) / 3.0;
            let enc = glass_work_space(backdrop_sample_blur(p1 - v_disp * k, sigma1));
            acc.g += (1.0 - k) * enc.g;
            acc.b += k * enc.b;
        }
        c = vec3<f32>(0.5 * acc.r, (1.0 / 3.0) * acc.g, 0.5 * acc.b);
    } else {
        c = glass_work_space(backdrop_sample_blur(p1, sigma1));
    }

    // §4.4: the haze — the capture at the pixel blurred by a Gaussian of
    // the haze radius's deviation, in the working space; runs before the
    // rim-sample mix.
    let radius = params[4].x;
    if radius > 0.0 {
        let f = glass_work_space(
            backdrop_sample_blur(p, HAZE_SIGMA_PT_PER_RADIUS * radius * tex_per_pt),
        );
        let c1 = params[4].y * min(c, f)
            + params[4].z * max(c, f)
            + (1.0 - params[4].y - params[4].z) * c;
        c = mix(c1, f, params[4].w);
    }

    // §1.1 / §4.2: the rim sample and the rim-ramp mix; a rim mix of 0
    // takes no sample.
    if params[1].z > 0.0 {
        let rim_span = max(params[9].y, 1e-4);
        let rim_weight = params[1].z * gsat(1.0 + d / rim_span);
        let o2 = lobe_at(d, params[0].w, params[0].z);
        let s_rim = glass_work_space(backdrop_sample_blur(
            p + n * (o2 * u),
            BLUR_SIGMA_PT_PER_RADIUS * params[1].w * blur_gain(params[2], params[3], d + o2)
                * tex_per_pt,
        ));
        c = mix(c, s_rim, rim_weight);
    }

    // §4.5: the luminance limit [fitted].
    let lum = dot(LUMA_LIMIT_W, c);
    let a = gsat(1.0 - lum * (1.0 - params[5].x));
    c = mix(vec3<f32>(a * lum), a * c, 1.0 + (1.0 - a) * 0.3);

    // §4.6: the grade — matrix, wash (`grade_mix = 1.0` folded).
    let graded = apply_grade(params[6].xyz, c);
    c = (1.0 - params[6].w) * graded + params[6].w * params[7].xyz;

    // §4.7: the ambient pickup — a sample pushed out past the contour,
    // graded, gated by the current luma (folded `ambient_ramp = (1, 0)`).
    let o_b = lobe_at(d, params[8].y, params[8].x);
    let ambient_sample = apply_grade(
        vec3<f32>(params[9].z, params[9].w, params[10].x),
        glass_work_space(backdrop_sample_blur(
            p + n * (o_b * u),
            ambient_sigma(params[8].z * k_tex),
        )),
    );
    let gate = dot(LUMA_GATE_W, c);
    let k = select(1.0 - gate, gate, params[7].w > 0.5);
    let band = gsat(1.0 - d);
    let g = band * k * k;
    let w = params[8].w * g * g;
    c = mix(c, ambient_sample, w);

    // §4.8: the contour shade — the band of the shifted field's falloff
    // profile lying just inside the shifted contour, limited to the glass
    // coverage by the clip. The shape weight `wR` is taken as 1 for every
    // member [chosen].
    if params[10].y > 0.0 {
        let f_d = geom_sdf_at(
            p - vec2<f32>(0.0, params[10].z * u),
            sibling_count,
            self_index,
            is_prominent,
            has_prominent_sib,
            u,
        ) / u;
        let alpha_r = params[10].y
            * gsat(
                shadow_profile(f_d / params[10].w)
                    - shadow_profile((f_d + params[11].x) / params[10].w),
            )
            * mix(1.0, cov, params[11].y);
        c = c * (1.0 - alpha_r);
    }

    return c;
}

// §4.9: lateral opposing lobe weight in [0, 1] around the exterior perimeter;
// strength and bearing are the recipe's shared constants, only the aperture
// rides in `params`.
fn border_lateral(n: vec2<f32>, aperture: f32) -> f32 {
    let u_b = vec2<f32>(sin(BORDER_BEARING), -cos(BORDER_BEARING));
    let cs = cos(0.5 * aperture);
    let q1 = gsat((dot(n, u_b) - cs) / (1.0 - cs));
    let q2 = gsat((dot(n, -u_b) - cs) / (1.0 - cs));
    return q1 / max(1.0 + BORDER_STRENGTH * (1.0 - q1), 1e-4)
        + q2 / max(1.0 + BORDER_STRENGTH * (1.0 - q2), 1e-4);
}

// The §4.12 two-light rim highlight plus the glow term — a second band
// of the same form at the glow multipliers. All terms but the aperture
// are the recipe's shared constants (see the header). This is the light
// the bands add, and `w` is 1 where the highlight applies (`d < 0.5`);
// `add_highlight` lays it over a colour.
fn highlight_light(d: f32, n: vec2<f32>, cov: f32, aperture: f32) -> vec4<f32> {
    let cs = cos(0.5 * aperture);
    let cs2 = cos(0.5 * aperture * HL_GLOW_APERTURE);
    let band_h = HIGHLIGHT_BAND;
    let band_h2 = HIGHLIGHT_BAND * HL_GLOW_WIDTH;
    // narrow band with §4.12 outer-lip taper so specular white does not bleed
    // onto straddling boundary pixels and wash out the exterior perimeter ring
    let s = gsat(-d / band_h);
    let lip = gsat(-d / 0.35);
    let band_narrow = (1.0 - HL_INNER_FADE * s) * (1.0 - s) * lip * cov;
    // glow band
    let s2 = gsat(-d / band_h2);
    let band_glow = (1.0 - HL_INNER_FADE * s2) * (1.0 - s2) * cov;
    let bearings = vec2<f32>(HL_FIRST_BEARING, HL_SECOND_BEARING);
    let gains = vec2<f32>(HL_FIRST_GAIN, HL_SECOND_GAIN);
    var c_narrow = vec3<f32>(0.0);
    var c_broad = vec3<f32>(0.0);
    for (var i = 0; i < 2; i += 1) {
        let u_l = vec2<f32>(sin(bearings[i]), -cos(bearings[i]));
        let q = gsat((dot(n, u_l) - cs) / (1.0 - cs));
        let lobe = q / (1.0 + HL_KNEE * (1.0 - q));
        let q2 = gsat((dot(n, u_l) - cs2) / (1.0 - cs2));
        let lobe2 = q2 / (1.0 + HL_KNEE * (1.0 - q2));
        let colour = select(HL_SECOND_COLOUR, HL_FIRST_COLOUR, i == 0);
        c_narrow += colour.rgb * colour.a * gains[i] * 2.0 * lobe;
        c_broad += colour.rgb * colour.a * gains[i] * HL_GLOW_GAIN * 2.0 * lobe2;
    }
    return vec4<f32>(c_narrow * band_narrow + c_broad * band_glow, select(0.0, 1.0, d < 0.5));
}

// Chroma-preserving cubic soft-light composite for the specular rim
// highlight `light`, keeping HDR highlights (> 1.0) unclamped.
fn add_highlight(c: vec3<f32>, light: vec4<f32>, k_mul: f32, k_add: f32) -> vec3<f32> {
    let c_soft = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
    let soft = max(
        c * (1.0 + k_mul * light.rgb * (3.0 - 2.0 * c_soft)) + k_add * light.rgb,
        vec3<f32>(0.0),
    );
    return select(c, soft, light.w > 0.5);
}

// §4.13: the tint layers — a fill of the shape, then an edge gradient at
// full strength from −1 to 0 pt that eases off to zero 10 pt inward. The
// tint's alpha is the effective one (the recipe's `Tint::declared`), and the
// gradient's gain follows the luma beneath it [chosen: keeps the current form].
// These are the fill's and the gradient's alphas, 0 without a tint or
// coverage; `add_tint` lays them over a colour.
fn tint_alphas(tint_a: f32, has_tint: f32, d: f32, cov: f32) -> vec2<f32> {
    let a_flat = tint_a;
    // A flat tint (flag 2, §8.1) takes no edge gradient.
    let edge = step(has_tint, 1.5);
    let t = gsat(-d / 10.0);
    let a_rim = edge * tint_a * (1.0 - ease_cubic(t));
    return select(vec2<f32>(0.0), vec2<f32>(a_flat, a_rim), has_tint >= 0.5 && cov > 0.0);
}

// The tint `rgb` laid over `c` at the alphas `a` (`tint_alphas`).
fn add_tint(c: vec3<f32>, rgb: vec3<f32>, a: vec2<f32>) -> vec3<f32> {
    let outc = rgb * a.x + c * (1.0 - a.x);
    let gain = 0.2 + 0.8 * luma(outc);
    return rgb * gain * a.y + outc * (1.0 - a.y);
}

// The entry the group pass calls. Returns premultiplied colour in the
// capture's storage space — linear Display P3 — which the member's composite
// blends in `BlendSpace::Linear` directly onto the linear Display P3 canvas.
fn backdrop_effect(px: BackdropPixel, params: array<vec4<f32>, 16>) -> vec4<f32> {
    let u = px.scale;
    let is_light = params[7].w > 0.5;
    let is_prominent = params[15].w > 1.5;
    let sibling_count = backdrop_sibling_count();
    let self_index = backdrop_sibling_index();
    var has_prominent_sib = false;
    if sibling_count > 1u && !is_prominent {
        for (var i = 0u; i < sibling_count; i = i + 1u) {
            if backdrop_sibling(i, px.p).emission.w < 0.0 {
                has_prominent_sib = true;
                break;
            }
        }
    }

    var geom_sdf = px.sdf;
    var geom_normal = px.normal;
    if sibling_count > 1u {
        if is_prominent {
            geom_sdf = px.own_sdf;
            geom_normal = backdrop_sibling(self_index, px.p).normal;
        } else if has_prominent_sib {
            let uf = fusing_union_field(px.p, sibling_count, u);
            geom_sdf = uf.x;
            geom_normal = uf.yz;
        }
    }

    let d = geom_sdf / u;
    // The field's screen-space rate from its unit outward normal in device
    // pixels (`|∂sdf/∂x| + |∂sdf/∂y|`): analytical rather than `fwidth`,
    // which is undefined after non-uniform early returns in the compositor.
    let fw = max(abs(geom_normal.x) + abs(geom_normal.y), 1e-4);

    // §4.10: the cast shadow — plain black, no capture read, at the
    // shifted field. A zero falloff gives ±inf inside `gsat`, the
    // standalone's hard-edged shadow, so only the density gates it.
    var cast_alpha = 0.0;
    if params[12].x > 0.0 {
        let f_s_sdf = geom_sdf_at(
            px.p - params[14].xy * u,
            sibling_count,
            self_index,
            is_prominent,
            has_prominent_sib,
            u,
        );
        cast_alpha = params[12].x * params[12].y
            * falloff(4.0 * gsat(0.25 * (f_s_sdf / u) / params[11].w + 0.5) - 2.0);
    }

    // Past the element's exterior span the member writes nothing (§4.11);
    // the shadow can still cover the pixel (`exterior_span > 0.5` everywhere,
    // so the 0.5 pt Light hairline is already 0 here).
    if d > params[5].w && cast_alpha < 1e-4 {
        return vec4<f32>(0.0);
    }

    // §2: the owning member's ellipse bend, applied to the field normal
    // the way the standalone's field pass bent it before shading.
    var n = geom_normal;
    if params[15].z > 0.0 {
        let rect = vec4<f32>(params[14].z, params[14].w, params[15].x, params[15].y);
        n = normalize(mix(n, ellipse_normal(rect, px.local), params[15].z));
    }

    let is_clear = params[12].y < 0.2;
    let is_light_regular = is_light && !is_clear && !is_prominent;
    let is_light_prominent = is_light && is_prominent;
    let h_lat = border_lateral(n, params[11].z);
    let band_ext = gsat(1.0 - abs(d - BORDER_BAND_OFFSET) / BORDER_BAND_WIDTH);
    let h_base = select(0.0, 0.19, is_light_regular);
    let border_alpha = select(band_ext * mix(h_base, 0.52, h_lat), 0.0, is_light_prominent);
    let ext_alpha = 1.0 - (1.0 - cast_alpha) * (1.0 - border_alpha);
    let stroke_cov = select(0.0, gsat(0.5 - (geom_sdf - 0.5 * u) / fw), is_light_prominent);

    // The silhouette coverage: a half-device-pixel ramp at the member's
    // effective field edge.
    let cov = gsat(0.5 - geom_sdf / fw);
    if cov <= 0.0 {
        if is_light_prominent && stroke_cov > 0.0 {
            let y_b = luma(glass_work_space(backdrop_sample(px.p)));
            let fill = params[13].rgb * (0.60 + 0.40 * y_b);
            let stroke_lin = space_srgb_decode(fill * (0.90 - 0.08 * h_lat));
            let a = stroke_cov + (1.0 - stroke_cov) * cast_alpha;
            return vec4<f32>(stroke_lin * stroke_cov, a);
        }
        return vec4<f32>(0.0, 0.0, 0.0, ext_alpha);
    }

    var c = glass_body(
        params,
        px.p,
        d,
        n,
        u,
        cov,
        sibling_count,
        self_index,
        is_prominent,
        has_prominent_sib,
    );
    // §4.11: the per-channel ceiling; display headroom is the target's.
    c = clamp(c, vec3<f32>(0.0), vec3<f32>(params[12].z));
    // §4.12: the highlight sibling.
    // §4.13: the tint siblings; a flat tint (§8.1) lies under the
    // highlight, every other tint over it. Each layer is computed and laid
    // over once: evaluating the layers' loops and branches once per order
    // made D3D12's WARP device render wrong texels and stall.
    let k = select(
        select(vec2<f32>(0.050, 0.190), vec2<f32>(0.125, 0.000), is_light),
        vec2<f32>(0.105, 0.038),
        is_prominent,
    );
    let raw_light = highlight_light(d, n, cov, params[12].w);
    let press_p = max(params[5].y, 0.0);
    let own_center = params[14].zw + params[15].xy * 0.5;
    let touch_pt = select(own_center, params[1].xy, params[9].x > 0.5);
    let touch_radius = max(0.5 * params[15].x, 40.0);
    let u_t = min(length(px.local - touch_pt) / touch_radius, 1.0);
    let phi = gsat(press_p) * (1.0 - 0.37 * u_t * u_t);
    let press_boost = 1.0 + 0.35 * phi;
    let light = vec4<f32>(raw_light.rgb * press_boost, raw_light.w);
    let tint = tint_alphas(params[13].a, params[15].w, d, cov);
    let y_b = luma(c);
    let prom_w = select(gsat(0.12 * (y_b - 0.16)), 0.40 * (1.0 - y_b), is_light);
    let fill = mix(params[13].rgb, vec3<f32>(select(1.0, 0.0, is_light)), prom_w);
    let pre = mix(c, fill, select(0.0, tint.x, is_prominent));
    let lit = add_highlight(pre, light, k.x, k.y);
    c = add_tint(lit, params[13].rgb, select(tint, vec2<f32>(0.0), is_prominent));
    if phi > 0.0 {
        let c_sat = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
        let lift_room = max(vec3<f32>(1.0) - c, vec3<f32>(0.0));
        c = c + phi * 0.22 * lift_room * (0.65 + 0.35 * c_sat);
    }

    // §3.2: cross-element emissive color bleed from tinted/prominent union
    // siblings within `BLEED_RADIUS = 36.0 pt` (`lambda_bleed = 12.0 pt`).
    if sibling_count > 1u {
        let headroom = max(vec3<f32>(1.0) - c, vec3<f32>(0.0));
        let edge_decay = exp(min(d, 0.0) / 8.0);
        var bleed = vec3<f32>(0.0);
        for (var i = 0u; i < sibling_count; i = i + 1u) {
            if i == self_index {
                continue;
            }
            let sib = backdrop_sibling(i, px.p);
            let e_j = abs(sib.emission.w);
            if e_j <= 0.0 {
                continue;
            }
            let d_j = sib.sdf / u;
            if d_j < 0.0 || d_j >= BLEED_RADIUS {
                continue;
            }
            let w_bleed = exp(-d_j / 12.0) * (1.0 - gsat(d_j / BLEED_RADIUS));
            let w_face = 0.35 + 0.65 * edge_decay * max(0.0, -dot(n, sib.normal));
            let c_j = space_srgb_encode(sib.emission.rgb);
            bleed += e_j * w_bleed * w_face * (c_j * headroom);
        }
        c += bleed;
    }

    // Convert Display P3-encoded straight colour directly back to extended
    // linear Display P3 with no sRGB gamut transform, preserving wide-gamut
    // primaries and unclamped HDR highlights (> 1.0).
    let c_lin = space_srgb_decode(c);
    if is_light_prominent {
        let ext_stroke = stroke_cov * (1.0 - cov);
        let ext_cast = (1.0 - stroke_cov) * cast_alpha * (1.0 - cov);
        let a = cov + ext_stroke + ext_cast;
        let stroke_lin = space_srgb_decode(fill * (0.90 - 0.08 * h_lat));
        return vec4<f32>(c_lin * cov + stroke_lin * ext_stroke, a);
    }
    let a = cov + (1.0 - cov) * ext_alpha;
    let atten = 1.0 - border_alpha * (1.0 - cov);
    return vec4<f32>(c_lin * atten * cov, a);
}
