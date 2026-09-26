// Backdrop capture: blit, separable pre-blur and the mip chain.
//
// `params.xy` is the source texel size in uv; `params.zw` the blur direction
// (1,0) or (0,1) in texels. Every pass draws a fullscreen triangle.

struct CaptureParams {
    params: vec4<f32>,
}

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;
@group(0) @binding(2) var<uniform> capture: CaptureParams;

// Bilinear copy; used to bring the backdrop into the composite and to
// downsample it to the capture scale (a box-class filter at 0.25 / 0.5).
@fragment
fn blit(in: FullscreenOut) -> @location(0) vec4<f32> {
    return textureSampleLevel(source, source_sampler, in.uv, 0.0);
}

// Downsample by four bilinear taps at the destination texel's quarter
// points: a 4x4 tent over the source, which keeps the chain smooth.
@fragment
fn downsample_box(in: FullscreenOut) -> @location(0) vec4<f32> {
    let t = capture.params.xy;
    var acc = vec4<f32>(0.0);
    acc += textureSampleLevel(source, source_sampler, in.uv + vec2<f32>(-0.5, -0.5) * t, 0.0);
    acc += textureSampleLevel(source, source_sampler, in.uv + vec2<f32>(0.5, -0.5) * t, 0.0);
    acc += textureSampleLevel(source, source_sampler, in.uv + vec2<f32>(-0.5, 0.5) * t, 0.0);
    acc += textureSampleLevel(source, source_sampler, in.uv + vec2<f32>(0.5, 0.5) * t, 0.0);
    return acc * 0.25;
}

// One direction of a gaussian with sigma = 1 capture texel, 7 taps.
@fragment
fn gaussian_1(in: FullscreenOut) -> @location(0) vec4<f32> {
    let step = capture.params.xy * capture.params.zw;
    let w0 = 0.398942;
    let w1 = 0.241971;
    let w2 = 0.053991;
    let w3 = 0.004432;
    let norm = w0 + 2.0 * (w1 + w2 + w3);
    var acc = textureSampleLevel(source, source_sampler, in.uv, 0.0) * w0;
    acc += textureSampleLevel(source, source_sampler, in.uv + step, 0.0) * w1;
    acc += textureSampleLevel(source, source_sampler, in.uv - step, 0.0) * w1;
    acc += textureSampleLevel(source, source_sampler, in.uv + 2.0 * step, 0.0) * w2;
    acc += textureSampleLevel(source, source_sampler, in.uv - 2.0 * step, 0.0) * w2;
    acc += textureSampleLevel(source, source_sampler, in.uv + 3.0 * step, 0.0) * w3;
    acc += textureSampleLevel(source, source_sampler, in.uv - 3.0 * step, 0.0) * w3;
    return acc / norm;
}
