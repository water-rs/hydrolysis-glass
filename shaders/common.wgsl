// Shared definitions, prepended to every pass of the glass material.

struct FullscreenOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// A single triangle covering the whole target; `uv` runs 0..1 across it.
@vertex
fn fullscreen_vertex(@builtin(vertex_index) index: u32) -> FullscreenOut {
    var out: FullscreenOut;
    let x = f32(i32(index & 1u) * 4 - 1);
    let y = f32(i32(index >> 1u) * 4 - 1);
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

fn sat(x: f32) -> f32 {
    return clamp(x, 0.0, 1.0);
}

// The meniscus profile mu(x; h) = 1 - sqrt(t (2 - t)), t = sat(x / h).
fn meniscus(x: f32, h: f32) -> f32 {
    if h <= 0.0 {
        return select(0.0, 1.0, x <= 0.0);
    }
    let t = sat(x / h);
    return 1.0 - sqrt(t * (2.0 - t));
}

const LUMA_WEIGHTS: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, LUMA_WEIGHTS);
}
