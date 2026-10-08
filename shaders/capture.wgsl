// Backdrop capture: the downsampled composite then the box 2x mip chain
// (§4.1).
//
// `params.xy` is the source texel size in uv for `downsample_box`;
// `params.z` is the explicit mip level for `blit`. Every pass draws a
// fullscreen triangle.

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
// points: a 2x2 box over the source, which keeps the chain smooth.
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
