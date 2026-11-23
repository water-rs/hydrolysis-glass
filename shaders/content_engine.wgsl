// The engine content effect shader (specification §4.14): isolated widget
// content over a glass body, displaced outward along the silhouette normal
// with the primary lobe's meniscus profile (scaled so content is refracted
// without backdrop magnification), split across seven spectral taps along a
// rotated dispersion axis, and faded to zero over the outer edge band.
//
// `content_sample` reads the layer's isolated subtree in premultiplied
// extended linear Display P3 and returns transparent outside its bounds;
// `content_effect` returns premultiplied extended linear Display P3.

const CONTENT_DISPERSION_AXIS_ANGLE: f32 = 0.35;
const CONTENT_DISPERSION_WIDTH_PT: f32 = 0.6;
const CONTENT_FOREGROUND_FADE_PT: f32 = 1.5;
const CONTENT_DISPLACEMENT_SCALE: f32 = 0.1;

fn content_sat(x: f32) -> f32 {
    return clamp(x, 0.0, 1.0);
}

// The meniscus profile mu(x; h) = 1 - sqrt(t (2 - t)), t = content_sat(x / h).
fn content_meniscus(x: f32, h: f32) -> f32 {
    if h <= 0.0 {
        return select(0.0, 1.0, x <= 0.0);
    }
    let t = content_sat(x / h);
    return 1.0 - sqrt(t * (2.0 - t));
}

// Per-channel spectral dispersion over the surface's isolated content.
//
// `params[0].xy` carries `(primary_shift, primary_depth)` in points.
fn content_effect(px: ContentPixel, params: array<vec4<f32>, 16>) -> vec4<f32> {
    let u = max(px.scale, 1e-4);
    let d = px.sdf / u;
    if d >= 0.0 {
        return vec4<f32>(0.0);
    }
    let primary_shift = params[0].x;
    let primary_depth = params[0].y;
    let profile = content_meniscus(-d, primary_depth);
    let disp_pt = -primary_shift * profile * CONTENT_DISPLACEMENT_SCALE;
    let base = px.p + px.normal * (disp_pt * u);
    // Dispersion axis rotated from the surface outward normal.
    let cos_a = cos(CONTENT_DISPERSION_AXIS_ANGLE);
    let sin_a = sin(CONTENT_DISPERSION_AXIS_ANGLE);
    let axis = vec2<f32>(
        px.normal.x * cos_a - px.normal.y * sin_a,
        px.normal.x * sin_a + px.normal.y * cos_a,
    );
    let width_px = CONTENT_DISPERSION_WIDTH_PT * profile * u;
    var acc_rgb = vec3<f32>(0.0);
    var cov_rgb = vec3<f32>(0.0);
    // Seven taps from -1..1 along the dispersion axis: R integrates the
    // outer positive window, G the centred window, B the negative window.
    for (var i = 0; i < 7; i = i + 1) {
        let t = (f32(i) - 3.0) / 3.0;
        let s = content_sample(base + axis * (t * width_px));
        let w = vec3<f32>(
            content_sat(1.0 - abs(t - 0.5) * 1.5),
            content_sat(1.0 - abs(t) * 1.5),
            content_sat(1.0 - abs(t + 0.5) * 1.5),
        );
        acc_rgb += s.rgb * w;
        cov_rgb += s.a * w;
    }
    // Each window sums to 2.0 across the seven taps; preserve premultiplied
    // bounds across dispersed RGB channels using peak channel coverage.
    let k = 0.5 * content_sat(-d / max(CONTENT_FOREGROUND_FADE_PT, 1e-3));
    let rgb = acc_rgb * k;
    let alpha = max(cov_rgb.r, max(cov_rgb.g, cov_rgb.b)) * k;
    return vec4<f32>(rgb, alpha);
}
