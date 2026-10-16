// shaders/material_engine.wgsl — the recipe's engine form (issue #2).
//
// `theme::materials` registers this source as a Cherenkov
// `BackdropShaderSource`; the backdrop-group pass invokes
// `backdrop_effect` once per member pixel. The contract —
// `BackdropPixel`, `backdrop_sample_level`, `backdrop_field`, the
// premultiplied return in the capture's colour space — is documented in
// cherenkov-record/src/backdrop.rs. The capture stores the canvas's
// space — premultiplied linear P3 — and the glass capture class declares
// `BlendSpace::SrgbEncoded`, so the engine converts the member's
// premultiplied result into encoded space and runs the source-over
// there: the specification's compositing space, exactly what the
// standalone `material.wgsl` computed. The taps therefore move each
// sample from linear P3 to the recipe's sRGB-encoded straight working
// space, and the return moves the premultiplied result back.
//
// The body is the standalone `shaders/material.wgsl` retargeted: field
// reads are `px.sdf`/`backdrop_field`, the capture pyramid is
// `backdrop_sample_level`, and the ellipse bend the standalone baked
// into its field texture is applied to `px.normal` here. The recipe's
// 23-vec4 layout does not fit the contract's sixteen parameters, so the
// packing keeps every recipe-varying field and folds the recipe's
// constant block — the highlight's shared terms and the border
// correction's measured constants — into shader constants that
// `theme::materials::uniforms` asserts (see its packing table).
//
// Parameter layout (must match `theme::materials::uniforms`):
//   p0  shifts          primary_shift, primary_depth, rim_shift, rim_depth
//   p1  rim             ramp r0, ramp r1, rim mix, blur radius
//   p2  blur_depths     d0..d3
//   p3  blur_gains      g0..g3
//   p4  haze            radius, min weight, max weight, mix
//   p5  limit/misc      luminance limit, grade mix, recipe capture scale, exterior span
//   p6  grade           floor, ceiling, chroma, wash alpha
//   p7  wash            wash r, g, b, ambient favours light
//   p8  ambient         shift, depth, blur radius, strength
//   p9  ambient misc    ramp e0, ramp e1, ambient grade floor, ambient grade ceiling
//   p10 ambient/contour ambient grade chroma, contour density, shift, softness
//   p11 contour/border  contour width, contour clip, border aperture, cast falloff
//   p12 cast/highlight  cast density, cast gain, channel ceiling, hl aperture
//   p13 tint            straight r, g, b, declared alpha
//   p14 cast/rect       cast shift x, cast shift y, member rect x, member rect y
//   p15 rect/bend       member rect w, member rect h, ellipse bend, has tint
//
// Distances the recipe calls pt arrive through `px.sdf`/`px.scale`;
// capture radii are recipe capture-texel counts, scaled onto the bound
// capture's grid by `backdrop_scale / p5.z` before the blur or
// `level_of_radius` reads them —
// a member's capture class snaps its recipe's requested scale up to the
// registered lattice (see `theme::materials`), so the compensation keeps
// every physical radius the recipe's. The member rect rides in
// member-local pt.

const PI: f32 = 3.141592653589793;
const EPS_ALPHA: f32 = 1e-4;
const HIGHLIGHT_BAND: f32 = 1.33;
const TINT_EFFECTIVE: f32 = 0.4;

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
const BORDER_BAND_OFFSET: f32 = -0.533;
const BORDER_COLOUR_BIAS: f32 = -0.3;

// ITU-R BT.709 luma; the limit and gate use the measured weights, which
// differ in the last digit.
const LUMA_WEIGHTS: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);
const LUMA_LIMIT_W: vec3<f32> = vec3<f32>(0.2126, 0.7153, 0.0722);
const LUMA_GATE_W: vec3<f32> = vec3<f32>(0.2125, 0.7153, 0.0721);

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

// A premultiplied linear-P3 sample to the recipe's straight sRGB-encoded
// working space. The engine module's own `srgb_encode_signed` and
// `P3_TO_SRGB` do the conversion.
fn glass_work_space(c: vec4<f32>) -> vec3<f32> {
    return srgb_encode_signed(P3_TO_SRGB * glass_unpremultiply(c));
}

// §3: the mip level the haze and the ambient sample read for a radius of
// `b` capture texels.
fn level_of_radius(b: f32) -> f32 {
    let x = select(b, 1.0 + b * 0.5, b < 2.0);
    return max(log2(x), 0.0);
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

// A capture tap at blur radius `b` capture texels. `q` is a device point;
// the capture's premultiplied linear P3 converts to the recipe's
// sRGB-encoded straight working space.
fn tap_at(q: vec2<f32>, lod: f32) -> vec3<f32> {
    return glass_work_space(backdrop_sample_level(q, lod));
}

// `b` is a radius in recipe capture texels; `k_tex` scales it onto the
// bound capture's own texel grid (`backdrop_scale / recipe scale`) so the
// physical blur radius in pt is the recipe's whatever class the member
// landed in.
fn tap(q: vec2<f32>, b: f32, k_tex: f32) -> vec3<f32> {
    return tap_at(q, level_of_radius(b * k_tex));
}

// The §3 blur of `sigma` level-0 texels of the bound capture
// (blur.wgsl), read on pyramid `level` around device point `q`: the
// premultiplied capture value, averaged in the capture's own space like
// the pyramid itself. The level-k grid is `(q · backdrop_scale −
// backdrop_origin) / 2^k` with texel centres at `+ 0.5`, the grid
// `backdrop_sample_level` reads.
fn glass_gaussian_at_level(q: vec2<f32>, sigma: f32, level: f32) -> vec4<f32> {
    let s = blur_deviation(sigma, level);
    if s < 1e-4 {
        return backdrop_sample_level(q, level);
    }
    let div = exp2(level);
    let p = (q * backdrop_scale - backdrop_origin) / div - 0.5;
    let base = floor(p);
    var acc = vec4<f32>(0.0);
    var total = 0.0;
    for (var j = 0; j < BLUR_PAIRS; j += 1) {
        for (var i = 0; i < BLUR_PAIRS; i += 1) {
            let w = 1.0;
            let at = base + vec2<f32>(f32(2 * i - BLUR_TAPS), f32(2 * j - BLUR_TAPS)) + 0.5;
            acc += w * backdrop_sample_level((at * div + backdrop_origin) / backdrop_scale, level);
            total += w;
        }
    }
    return acc / total;
}

// §4.3: a tap blurred by a radius of `b` recipe capture texels; `k_tex`
// carries the radius onto the bound capture's grid (see `tap`).
fn blurred_tap(q: vec2<f32>, b: f32, k_tex: f32) -> vec3<f32> {
    let sigma = BLUR_SIGMA_PER_RADIUS * b * k_tex;
    let t = blur_position(sigma, backdrop_levels - 1.0);
    let level = floor(t);
    var raw = glass_gaussian_at_level(q, sigma, level);
    let f = t - level;
    if f > 0.0 {
        raw = mix(raw, glass_gaussian_at_level(q, sigma, level + 1.0), f);
    }
    return glass_work_space(raw);
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
) -> vec3<f32> {
    // Recipe capture texels onto the bound capture's grid: the class's
    // scale may sit above the recipe's requested scale (classes are
    // registered, recipes are continuous — see `theme::materials`), so a
    // radius of `b` recipe texels is `b · backdrop_scale / recipe scale`
    // texels here, keeping the physical blur radius in pt the recipe's.
    let k_tex = backdrop_scale / params[5].z;

    // §4.2: the primary sample, refracted by the primary lobe and blurred
    // at its refracted depth.
    let o1 = lobe_at(d, params[0].y, params[0].x);
    var c = blurred_tap(
        p + n * (o1 * u),
        params[1].w * blur_gain(params[2], params[3], d + o1),
        k_tex,
    );

    // §4.4: the haze — two taps at the level of the haze radius,
    // diagonally opposite by 0.25·lod capture texels per axis, averaged
    // and unpremultiplied; runs before the rim-sample mix. The tap
    // offset is a physical distance the recipe's grid fixes, so it is
    // the recipe's lod over the recipe's scale — `px.scale` cancels out.
    let radius = params[4].x;
    if radius > 0.0 {
        let lod = level_of_radius(radius * k_tex);
        let t = vec2<f32>(0.25 * level_of_radius(radius) / params[5].z);
        let raw = 0.5 * (backdrop_sample_level(p + t, lod)
            + backdrop_sample_level(p - t, lod));
        let f = glass_work_space(raw);
        let c1 = params[4].y * min(c, f)
            + params[4].z * max(c, f)
            + (1.0 - params[4].y - params[4].z) * c;
        c = mix(c1, f, params[4].w);
    }

    // §4.2: the rim sample and the rim-ramp mix; a rim mix of 0 takes no
    // sample.
    if params[1].z > 0.0 {
        let o2 = lobe_at(d, params[0].w, params[0].z);
        let s_rim = blurred_tap(
            p + n * (o2 * u),
            params[1].w * blur_gain(params[2], params[3], d + o2),
            k_tex,
        );
        let tw = gsat((d - params[1].x) / (params[1].y - params[1].x));
        c = mix(c, s_rim, params[1].z * tw);
    }

    // §4.5: the luminance limit [fitted].
    let lum = dot(LUMA_LIMIT_W, c);
    let a = gsat(1.0 - lum * (1.0 - params[5].x));
    c = mix(vec3<f32>(a * lum), a * c, 1.0 + (1.0 - a) * 0.3);

    // §4.6: the grade — matrix, wash, composite by the grade mix.
    let graded = apply_grade(params[6].xyz, c);
    c = mix(
        c,
        (1.0 - params[6].w) * graded + params[6].w * params[7].xyz,
        params[5].y,
    );

    // §4.7: the ambient pickup — a sample pushed out past the contour,
    // graded, gated by the current luma; not clamped.
    let o_b = lobe_at(d, params[8].y, params[8].x);
    let ambient_sample = apply_grade(
        vec3<f32>(params[9].z, params[9].w, params[10].x),
        tap(p + n * (o_b * u), params[8].z, k_tex),
    );
    let gate = dot(LUMA_GATE_W, c);
    let k = select(1.0 - gate, gate, params[7].w > 0.5);
    let band = gsat((d - params[9].x) / (params[9].y - params[9].x));
    let g = band * k * k;
    let w = params[8].w * g * g;
    c = mix(c, ambient_sample, w);

    // §4.8: the contour shade — the band of the shifted field's falloff
    // profile lying just inside the shifted contour, limited to the glass
    // coverage by the clip. The shape weight `wR` is taken as 1 for every
    // member [chosen].
    if params[10].y > 0.0 {
        let f_r = backdrop_field(p - vec2<f32>(0.0, params[10].z * u));
        let f_d = f_r.sdf / u;
        let alpha_r = params[10].y
            * gsat(
                shadow_profile(f_d / params[10].w)
                    - shadow_profile((f_d + params[11].x) / params[10].w),
            )
            * mix(1.0, cov, params[11].y);
        c = c * (1.0 - alpha_r);
    }

    // §4.9: the border correction — two opposing edge lobes around the
    // rim; the strength, bearing, band and bias are the recipe's shared
    // constants (see the header), only the aperture rides in `params`.
    let u_b = vec2<f32>(sin(BORDER_BEARING), -cos(BORDER_BEARING));
    let cs = cos(0.5 * params[11].z);
    let band_b = gsat(1.0 - abs(d - BORDER_BAND_OFFSET) / BORDER_BAND_WIDTH);
    let q1 = gsat((dot(n, u_b) - cs) / (1.0 - cs));
    let q2 = gsat((dot(n, -u_b) - cs) / (1.0 - cs));
    let h = band_b * (q1 / max(1.0 + BORDER_STRENGTH * (1.0 - q1), 1e-4)
        + q2 / max(1.0 + BORDER_STRENGTH * (1.0 - q2), 1e-4));
    c = c * (1.0 + BORDER_COLOUR_BIAS * h * (3.0 - 2.0 * c));

    return c;
}

// The §4.12 two-light rim highlight plus the glow term — a second band
// of the same form at the glow multipliers. All terms but the aperture
// are the recipe's shared constants (see the header).
fn highlight(c: vec3<f32>, d: f32, n: vec2<f32>, cov: f32, aperture: f32) -> vec3<f32> {
    if d >= 0.5 {
        return c;
    }
    let cs = cos(0.5 * aperture);
    let cs2 = cos(0.5 * aperture * HL_GLOW_APERTURE);
    let band_h = HIGHLIGHT_BAND;
    let band_h2 = HIGHLIGHT_BAND * HL_GLOW_WIDTH;
    // narrow band
    let s = gsat(-d / band_h);
    let band_narrow = (1.0 - HL_INNER_FADE * s) * (1.0 - s) * cov;
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
    // The highlight adds to the accumulated colour under it; its gain
    // follows that colour's luma [chosen: keeps the current form].
    let gain = 0.2 + 0.8 * luma(c);
    return min(c + (c_narrow * band_narrow + c_broad * band_glow) * gain, vec3<f32>(1.0));
}

// §4.13: the tint layers — a fill of the shape, then an edge gradient at
// full strength from −1 to 0 pt that eases off to zero 10 pt inward. The
// effective alpha is 0.4 × declared [chosen], and the gradient's gain
// follows the luma beneath it [chosen: keeps the current form].
fn tint_layers(c: vec3<f32>, tint: vec4<f32>, has_tint: f32, d: f32, cov: f32) -> vec3<f32> {
    if has_tint < 0.5 || cov <= 0.0 {
        return c;
    }
    let a_flat = TINT_EFFECTIVE * tint.a * cov;
    var outc = tint.rgb * a_flat + c * (1.0 - a_flat);
    let t = gsat(-d / 10.0);
    let a_rim = TINT_EFFECTIVE * tint.a * (1.0 - ease_cubic(t)) * cov;
    let gain = 0.2 + 0.8 * luma(outc);
    outc = tint.rgb * gain * a_rim + outc * (1.0 - a_rim);
    return outc;
}

// The entry the group pass calls. Returns premultiplied colour in the
// capture's storage space — linear P3 — which the member's composite
// converts into the class's declared blend space (`SrgbEncoded`, the
// specification's compositing space) and source-overs over the canvas:
// `move_space(res·cov) + canvas·(1 − res.a)` is the standalone's
// `res + backdrop·(1 − res.a)` in encoded space.
fn backdrop_effect(px: BackdropPixel, params: array<vec4<f32>, 16>) -> vec4<f32> {
    let u = px.scale;
    let d = px.sdf / u;

    // §4.10: the cast shadow — plain black, no capture read, at the
    // shifted field. A zero falloff gives ±inf inside `gsat`, the
    // standalone's hard-edged shadow, so only the density gates it.
    var cast_alpha = 0.0;
    if params[12].x > 0.0 {
        let f_s = backdrop_field(px.p - params[14].xy * u);
        cast_alpha = params[12].x * params[12].y
            * falloff(4.0 * gsat(0.25 * (f_s.sdf / u) / params[11].w + 0.5) - 2.0);
    }

    // Past the element's exterior span the member writes nothing (§4.11);
    // the shadow can still cover the pixel.
    if d > params[5].w && cast_alpha < 1e-4 {
        return vec4<f32>(0.0);
    }

    // The silhouette coverage: a half-device-pixel ramp at the fused
    // field's edge. The composite's own coverage is the outer band's
    // (`w_own · AA(field < outer)`), so the silhouette's ramp is
    // evaluated here, as the standalone did on its field texture.
    let cov = gsat(0.5 - px.sdf / fwidth(px.sdf));
    if cov <= 0.0 {
        return vec4<f32>(0.0, 0.0, 0.0, cast_alpha);
    }

    // §2: the owning member's ellipse bend, applied to the field normal
    // the way the standalone's field pass bent it before shading.
    var n = px.normal;
    if params[15].z > 0.0 {
        let rect = vec4<f32>(params[14].z, params[14].w, params[15].x, params[15].y);
        n = normalize(mix(n, ellipse_normal(rect, px.local), params[15].z));
    }

    var c = glass_body(params, px.p, d, n, u, cov);
    // §4.11: the per-channel ceiling; display headroom is the target's.
    c = clamp(c, vec3<f32>(0.0), vec3<f32>(params[12].z));
    // §4.12: the highlight sibling.
    c = highlight(c, d, n, cov, params[12].w);
    // §4.13: the tint siblings.
    c = tint_layers(c, params[13], params[15].w, d, cov);

    // The standalone's `res = mix(shadow, interior, cov)` is the
    // premultiplied `(c·cov, a)` in encoded space. The member composite
    // recovers it exactly from a premultiplied linear-P3 return whose
    // straight is `c·cov/a` — `a ≥ cov > 0` inside this branch, so the
    // divide is safe — because `move_space` round-trips
    // `SRGB_TO_P3`/`srgb_decode_signed` back to that straight before
    // blending.
    let a = cov + (1.0 - cov) * cast_alpha;
    let straight = SRGB_TO_P3 * srgb_decode_signed(c * (cov / a));
    return vec4<f32>(straight * a, a);
}
