// The background effect plus its sibling layers (edge highlight, tint).
//
// Inputs: the group's field texture, the backdrop composite, the group's
// blurred+mipped captures and one recipe per member. Output: the backdrop
// with the composited glass.
//
// All colour math runs on premultiplied, sRGB-encoded values, unpremultiplied
// around the colour matrices as the specification describes.

struct Recipe {
    disp_mat: vec4<f32>,      // m00, m01, m10, m11
    lobes: vec4<f32>,         // innerAmt, innerH, outerAmt, outerH
    refr: vec4<f32>,          // refrThr0, refrThr1, refrOp, blurR
    blur_dist: vec4<f32>,     // blurDist[0..3]
    blur_op: vec4<f32>,       // blurOp[0..3]
    blur_tail: vec4<f32>,     // blurDist[4], blurOp[4], captureScale, captureIndex
    face_bws: vec4<f32>,      // faceB, faceW, faceSat, faceOp
    face_fill: vec4<f32>,     // premultiplied
    bleed: vec4<f32>,         // bleedAmt, bleedH, bleedBlurR, bleedOp
    bleed_misc: vec4<f32>,    // bleedDist0, bleedDist1, bleedDarken, -
    bleed_bws: vec4<f32>,     // B, W, S, -
    shadow_a: vec4<f32>,      // offsetX, offsetY, shadowAmt, shadowH
    shadow_b: vec4<f32>,      // shadowBlurR, shadowRadius, shadowOp, shadowContrib
    shadow_bws: vec4<f32>,    // B, W, S, -
    shadow_fill: vec4<f32>,   // premultiplied
    sdr: vec4<f32>,           // sdrDist0, sdrDist1, sdrOp, sdrWhite
    output: vec4<f32>,        // maxHeadroom, preserveHue, edrScale, positiveRange
    flags: vec4<f32>,         // materialEnabled, lumaTracking, hasTint, -
    hl_a: vec4<f32>,          // curvature, keyAmount, fillAmount, spread
    hl_b: vec4<f32>,          // keyAngle, fillAngle, -, -
    hl_key: vec4<f32>,        // straight rgba
    hl_fill: vec4<f32>,       // straight rgba
    tint: vec4<f32>,          // straight rgb, declared alpha
}

struct MaterialUniforms {
    // scene width pt, scene height pt, px per pt, regular capture max lod
    scene: vec4<f32>,
    // clear capture max lod, -, -, -
    misc: vec4<f32>,
}

@group(0) @binding(0) var<uniform> uni: MaterialUniforms;
@group(0) @binding(1) var<storage, read> recipes: array<Recipe>;
@group(0) @binding(2) var field_tex: texture_2d<f32>;
@group(0) @binding(3) var backdrop_tex: texture_2d<f32>;
@group(0) @binding(4) var capture_regular: texture_2d<f32>;
@group(0) @binding(5) var capture_clear: texture_2d<f32>;
@group(0) @binding(6) var linear_sampler: sampler;

const EPS_ALPHA: f32 = 1e-4;
const HIGHLIGHT_BAND: f32 = 1.33;
const TINT_EFFECTIVE: f32 = 0.4;

// ---------------------------------------------------------------- field ---

struct FieldSample {
    d: f32,
    n: vec2<f32>,
    owner: u32,
}

fn load_field(px: vec2<f32>) -> FieldSample {
    let dims = vec2<i32>(textureDimensions(field_tex));
    let c = clamp(vec2<i32>(floor(px)), vec2<i32>(0), dims - vec2<i32>(1));
    let v = textureLoad(field_tex, c, 0);
    var s: FieldSample;
    s.d = v.x;
    s.n = v.yz;
    s.owner = u32(max(v.w, 0.0));
    return s;
}

// ------------------------------------------------------------- sampling ---

// Samples the capture at `lod = log2(radius)`; radius in capture texels.
// Negative LODs are finer than the capture: they blend from mip 0 towards
// the full-resolution backdrop, which sits log2(1/scale) levels below mip 0
// (2 for the regular capture, 1 for clear). This is what lets the blur dip
// at the silhouette keep grid lines readable.
fn sample_capture(r: Recipe, q_pt: vec2<f32>, radius_texels: f32) -> vec4<f32> {
    let uv = q_pt / uni.scene.xy;
    let lod = log2(max(radius_texels, 1e-3));
    var captured: vec4<f32>;
    if r.blur_tail.w > 0.5 {
        captured = textureSampleLevel(capture_clear, linear_sampler, uv, clamp(lod, 0.0, uni.misc.x));
    } else {
        captured = textureSampleLevel(capture_regular, linear_sampler, uv, clamp(lod, 0.0, uni.scene.w));
    }
    if lod >= 0.0 {
        return captured;
    }
    let levels_below = -log2(r.blur_tail.z);
    let fine = sat(-lod / max(levels_below, 1e-3));
    return mix(captured, textureSampleLevel(backdrop_tex, linear_sampler, uv, 0.0), fine);
}

fn sample_backdrop(q_pt: vec2<f32>) -> vec4<f32> {
    let uv = q_pt / uni.scene.xy;
    return textureSampleLevel(backdrop_tex, linear_sampler, uv, 0.0);
}

fn piecewise(r: Recipe, d: f32) -> f32 {
    let xs = array<f32, 5>(r.blur_dist.x, r.blur_dist.y, r.blur_dist.z, r.blur_dist.w, r.blur_tail.x);
    let ys = array<f32, 5>(r.blur_op.x, r.blur_op.y, r.blur_op.z, r.blur_op.w, r.blur_tail.y);
    if d <= xs[0] {
        return ys[0];
    }
    for (var i = 0; i < 4; i += 1) {
        if d < xs[i + 1] {
            let span = xs[i + 1] - xs[i];
            if span <= 1e-6 {
                return ys[i + 1];
            }
            let t = (d - xs[i]) / span;
            return ys[i] + t * (ys[i + 1] - ys[i]);
        }
    }
    return ys[4];
}

fn blur_radius(r: Recipe, d: f32) -> f32 {
    return r.refr.w * piecewise(r, d);
}

// --------------------------------------------------------------- colour ---

fn unpremultiply(c: vec4<f32>) -> vec3<f32> {
    let a = max(c.a, EPS_ALPHA);
    var s = c.rgb / a;
    s = select(s, vec3<f32>(0.0), abs(s) < vec3<f32>(EPS_ALPHA));
    return s;
}

fn bws_matrix(bws: vec3<f32>, c: vec3<f32>) -> vec3<f32> {
    let y = luma(c);
    let base = bws.x + y * (bws.y - bws.x);
    return vec3<f32>(base) + bws.z * (c - vec3<f32>(y));
}

// Matrix + folded fill on a premultiplied sample -> premultiplied result.
fn apply_matrix(bws: vec3<f32>, fill: vec4<f32>, c: vec4<f32>) -> vec4<f32> {
    let s = unpremultiply(c);
    let out = bws_matrix(bws, s);
    let keep = 1.0 - c.a;
    return vec4<f32>(out * c.a + fill.rgb * keep, c.a + fill.a * keep);
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

struct Stage {
    edge: vec4<f32>,
    inner: vec4<f32>,
}

fn interior(r: Recipe, p: vec2<f32>, d: f32, n: vec2<f32>, fw: f32) -> vec4<f32> {
    let radius = blur_radius(r, d);
    if r.flags.x < 0.5 {
        let s = sample_capture(r, p, radius);
        return vec4<f32>(s.rgb, 1.0);
    }
    // Inner lobe: outward reach -innerAmt * mu(-d; innerH).
    let o1 = -r.lobes.x * meniscus(-d, r.lobes.y);
    let s1 = sample_capture(r, p + n * o1, radius);
    // Outer lobe: inward reach outerAmt * mu(-d; outerH) in the sliver.
    let o2 = -r.lobes.z * meniscus(-d, r.lobes.w);
    let s2 = sample_capture(r, p + n * o2, radius);
    let sliver = sat((d - r.refr.x) / fw + 0.5) * sat((r.refr.y - d) / fw + 0.5);
    let refracted = mix(s1, s2, r.refr.z * sliver);
    let faced = apply_matrix(r.face_bws.xyz, r.face_fill, refracted);
    let mixed = mix(refracted, faced, r.face_bws.w);
    return vec4<f32>(mixed.rgb, 1.0);
}

fn edge_term(r: Recipe, p: vec2<f32>, d: f32, n: vec2<f32>, px_per_pt: f32) -> vec4<f32> {
    var edge = vec4<f32>(0.0);
    // Shadow: the field of the shape displaced by shadowOffset.
    let shifted = load_field((p - r.shadow_a.xy) * px_per_pt);
    let ds = shifted.d;
    if r.shadow_b.z > 0.0 {
        let ns = shifted.n;
        let os = r.shadow_a.z * meniscus(-ds, r.shadow_a.w);
        let radius = max(r.shadow_b.x, blur_radius(r, d));
        let sample = sample_capture(r, p + ns * os, radius);
        let matrixed = apply_matrix(r.shadow_bws.xyz, vec4<f32>(0.0), sample);
        var fill_rgb = vec3<f32>(0.0);
        if r.shadow_fill.a > EPS_ALPHA {
            fill_rgb = r.shadow_fill.rgb / r.shadow_fill.a;
        }
        let shadow_color = mix(vec4<f32>(fill_rgb, 1.0), vec4<f32>(matrixed.rgb, 1.0), r.shadow_b.w);
        let falloff = meniscus(ds, 2.0 * r.shadow_b.y);
        edge = shadow_color * falloff * r.shadow_b.z;
    }
    // Bleed: outward displaced deep-blurred sample, luma gated.
    if r.bleed.w > 0.0 {
        let ramp = 1.0 - sat((d - r.bleed_misc.y) / max(r.bleed_misc.x - r.bleed_misc.y, 1e-4));
        let ob = r.bleed.x * meniscus(-d, r.bleed.y);
        let radius = max(r.bleed.z, blur_radius(r, d));
        let sample = sample_capture(r, p + n * ob, radius);
        let local = sample_capture(r, p, blur_radius(r, d));
        var y = luma(unpremultiply(local));
        if r.bleed_misc.z > 0.5 {
            y = 1.0 - y;
        }
        let gate = smoothstep(0.25, 0.75, y);
        let bleed_color = apply_matrix(r.bleed_bws.xyz, vec4<f32>(0.0), sample);
        edge = mix(edge, vec4<f32>(bleed_color.rgb, 1.0), r.bleed.w * ramp * gate);
    }
    return edge;
}

fn clamp_headroom(r: Recipe, res: vec4<f32>) -> vec4<f32> {
    let max_headroom = r.output.x;
    if max_headroom >= 9998.0 {
        return res;
    }
    if r.output.y > 0.5 {
        let m = max(max(res.r, res.g), res.b);
        if m > max_headroom {
            return vec4<f32>(res.rgb * (max_headroom / m), res.a);
        }
        return res;
    }
    return vec4<f32>(clamp(res.rgb, vec3<f32>(-0.75), vec3<f32>(max_headroom)), res.a);
}

fn highlight(r: Recipe, out: vec3<f32>, d: f32, n: vec2<f32>, cov: f32) -> vec3<f32> {
    if d >= 0.5 {
        return out;
    }
    let s = sat(-d / HIGHLIGHT_BAND);
    // Weight falls with curvature across the band and is closed at the
    // band's inner edge so the flat interior receives none.
    let band = (1.0 - r.hl_a.x * s) * (1.0 - s) * cov;
    if band <= 0.0 {
        return out;
    }
    // Angle convention: measured to the light in a y-up frame.
    let u_key = vec2<f32>(-cos(r.hl_b.x), sin(r.hl_b.x));
    let u_fill = vec2<f32>(-cos(r.hl_b.y), sin(r.hl_b.y));
    // `spread` is the full cone width of a lobe.
    let cs = cos(0.5 * r.hl_a.w);
    let denom = max(1.0 - cs, 1e-4);
    var key = sat((dot(n, u_key) - cs) / denom);
    var fill = sat((dot(n, u_fill) - cs) / denom);
    key = key / (1.0 + r.hl_a.y * (1.0 - key));
    fill = fill / (1.0 + r.hl_a.z * (1.0 - fill));
    let key_c = r.hl_key.rgb * r.hl_key.a * r.hl_a.y * 2.0 * key;
    let fill_c = r.hl_fill.rgb * r.hl_fill.a * r.hl_a.z * 2.0 * fill;
    // "Vibrant" composite approximation: the rim is a gain on what lies
    // beneath, so it is dimmer over dark content.
    let gain = 0.2 + 0.8 * luma(out);
    return min(out + (key_c + fill_c) * band * gain, vec3<f32>(1.0));
}

fn tint_layers(r: Recipe, out: vec3<f32>, d: f32, cov: f32) -> vec3<f32> {
    if r.flags.z < 0.5 || cov <= 0.0 {
        return out;
    }
    let a_flat = TINT_EFFECTIVE * r.tint.a * cov;
    var c = r.tint.rgb * a_flat + out * (1.0 - a_flat);
    // Rim gradient: full between -1 and 0 pt, eased off over 10 pt inward.
    let t = sat((-d - 1.0) / 10.0);
    let g = (1.0 - ease_cubic(t)) * cov;
    let a_rim = TINT_EFFECTIVE * r.tint.a * g;
    let gain = 0.2 + 0.8 * luma(c);
    c = r.tint.rgb * gain * a_rim + c * (1.0 - a_rim);
    return c;
}

@fragment
fn material_fragment(in: FullscreenOut) -> @location(0) vec4<f32> {
    let px_per_pt = uni.scene.z;
    let fw = 1.0 / px_per_pt;
    let p = in.position.xy / px_per_pt;
    let backdrop = sample_backdrop(p);
    let f = load_field(in.position.xy);
    let r = recipes[f.owner];
    let d = f.d;
    if d > r.output.w {
        return backdrop;
    }
    let m = r.disp_mat;
    var n = vec2<f32>(m.x * f.n.x + m.y * f.n.y, m.z * f.n.x + m.w * f.n.y);
    if length(n) > 1e-6 {
        n = normalize(n);
    }
    let cov = sat(0.5 - d / max(fw, 1e-6));

    var edge = vec4<f32>(0.0);
    if cov < 1.0 {
        edge = edge_term(r, p, d, n, px_per_pt);
    }
    var inner = vec4<f32>(0.0);
    if cov > 0.0 {
        inner = interior(r, p, d, n, fw);
    }
    var res = mix(edge, inner, cov);

    // SDR holding band.
    let band = sat((d - r.sdr.x) / fw + 0.5) * sat((r.sdr.y - d) / fw + 0.5);
    if band > 0.0 && res.a > EPS_ALPHA {
        let held = vec4<f32>(res.rgb * (r.sdr.w * sat(res.a) / res.a), res.a);
        res = mix(res, held, r.sdr.z * band);
    }
    res = clamp_headroom(r, res);
    res = vec4<f32>(res.rgb * r.output.z, res.a);

    var out = res + backdrop * (1.0 - res.a);
    out = vec4<f32>(highlight(r, out.rgb, d, n, cov), out.a);
    out = vec4<f32>(tint_layers(r, out.rgb, d, cov), out.a);
    return out;
}
