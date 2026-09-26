// Backdrop capture: blit, separable pre-blur and the mip chain.
//
// `params.xy` is the source texel size in uv for `downsample_box` and
// `gaussian_1` reads its blur direction from `params.zw` ((1,0) or (0,1) in
// texels); `params.z` is the explicit mip level for `blit`. Every pass draws
// a fullscreen triangle.

struct CaptureParams {
    params: vec4<f32>,
    // Source uv rect (origin.xy, span.zw): the group's region for the
    // composite-sampling first step, identity elsewhere.
    rect: vec4<f32>,
}

fn src_uv(uv: vec2<f32>) -> vec2<f32> {
    return capture.rect.xy + uv * capture.rect.zw;
}

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;
@group(0) @binding(2) var<uniform> capture: CaptureParams;

// Bilinear copy at an explicit mip level (`params.z`): the backdrop blit
// stays at level 0; the capture's first step reads the composite's own mip
// pyramid so the 4x-or-larger footprint is prefiltered by hardware instead
// of aliasing a periodic backdrop.
@fragment
fn blit(in: FullscreenOut) -> @location(0) vec4<f32> {
    return textureSampleLevel(source, source_sampler, src_uv(in.uv), capture.params.z);
}

// Downsample by four bilinear taps at the destination texel's quarter
// points: a 4x4 tent over the source, which keeps the chain smooth.
@fragment
fn downsample_box(in: FullscreenOut) -> @location(0) vec4<f32> {
    let t = capture.params.xy;
    let uv = src_uv(in.uv);
    var acc = vec4<f32>(0.0);
    acc += textureSampleLevel(source, source_sampler, uv + vec2<f32>(-0.5, -0.5) * t, 0.0);
    acc += textureSampleLevel(source, source_sampler, uv + vec2<f32>(0.5, -0.5) * t, 0.0);
    acc += textureSampleLevel(source, source_sampler, uv + vec2<f32>(-0.5, 0.5) * t, 0.0);
    acc += textureSampleLevel(source, source_sampler, uv + vec2<f32>(0.5, 0.5) * t, 0.0);
    return acc * 0.25;
}

// One direction of a gaussian with sigma = 1 capture texel, 7 taps.
@fragment
fn gaussian_1(in: FullscreenOut) -> @location(0) vec4<f32> {
    let step = capture.params.xy * capture.params.zw;
    let uv = src_uv(in.uv);
    let w0 = 0.398942;
    let w1 = 0.241971;
    let w2 = 0.053991;
    let w3 = 0.004432;
    let norm = w0 + 2.0 * (w1 + w2 + w3);
    var acc = textureSampleLevel(source, source_sampler, uv, 0.0) * w0;
    acc += textureSampleLevel(source, source_sampler, uv + step, 0.0) * w1;
    acc += textureSampleLevel(source, source_sampler, uv - step, 0.0) * w1;
    acc += textureSampleLevel(source, source_sampler, uv + 2.0 * step, 0.0) * w2;
    acc += textureSampleLevel(source, source_sampler, uv - 2.0 * step, 0.0) * w2;
    acc += textureSampleLevel(source, source_sampler, uv + 3.0 * step, 0.0) * w3;
    acc += textureSampleLevel(source, source_sampler, uv - 3.0 * step, 0.0) * w3;
    return acc / norm;
}
