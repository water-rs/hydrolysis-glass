// The foreground pass: content drawn inside a glass element, displaced along
// the surface normal with the meniscus profile, split into seven spectral
// taps along a rotated aberration axis and faded out over the last ~1.5 pt
// before the silhouette. Composited source-over onto the material result
// per channel, so each channel carries its own coverage: black text on a
// light glass body fringes into colour at the rim like coloured content does.

struct ForegroundUniforms {
    // scene width pt, scene height pt, px per pt, aberration axis angle (rad)
    scene: vec4<f32>,
    // dispersion width (pt at the silhouette), edge fade band (pt),
    // region origin px
    params: vec4<f32>,
}

struct Lobe {
    // primary lobe shift, primary lobe depth, -, -
    lobe: vec4<f32>,
}

@group(0) @binding(0) var<uniform> fg: ForegroundUniforms;
@group(0) @binding(1) var<storage, read> lobes: array<Lobe>;
@group(0) @binding(2) var field_tex: texture_2d<f32>;
@group(0) @binding(3) var content_tex: texture_2d<f32>;
@group(0) @binding(4) var linear_sampler: sampler;
@group(0) @binding(5) var under_tex: texture_2d<f32>;

fn sample_content(q_pt: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(content_tex, linear_sampler, q_pt / fg.scene.xy, 0.0);
}

@fragment
fn foreground_fragment(in: FullscreenOut) -> @location(0) vec4<f32> {
    let px_per_pt = fg.scene.z;
    // The pass draws on a region-sized patch; the field and under textures
    // share the region, so texel indices are local. `p` is scene pt.
    let p = (in.position.xy + fg.params.zw) / px_per_pt;
    let dims = vec2<i32>(textureDimensions(field_tex));
    let c = clamp(vec2<i32>(floor(in.position.xy)), vec2<i32>(0), dims - vec2<i32>(1));
    let v = textureLoad(field_tex, c, 0);
    let d = v.x;
    let n = v.yz;
    let owner = u32(max(v.w, 0.0));
    let under = textureLoad(under_tex, c, 0);
    if d >= 0.5 / px_per_pt {
        return under;
    }
    let lobe = lobes[owner].lobe;
    // Content displacement follows the material's primary lobe, scaled down:
    // the content is not magnified the way the backdrop is.
    let profile = meniscus(-d, lobe.y);
    let disp = -lobe.x * profile * 0.1;
    let base = p + n * disp;
    // Rotated aberration axis relative to the normal.
    let ang = fg.scene.w;
    let axis = vec2<f32>(n.x * cos(ang) - n.y * sin(ang), n.x * sin(ang) + n.y * cos(ang));
    let width = fg.params.x * profile;
    var acc = vec4<f32>(0.0);
    var cov_rgb = vec3<f32>(0.0);
    // Seven taps from -1..1 along the axis; each channel integrates a
    // different window of taps (R the outer positive side, B the negative,
    // G the middle) and keeps its own coverage.
    for (var i = 0; i < 7; i += 1) {
        let t = (f32(i) - 3.0) / 3.0;
        let s = sample_content(base + axis * (t * width));
        let w = vec3<f32>(
            sat(1.0 - abs(t - 0.5) * 1.5),
            sat(1.0 - abs(t) * 1.5),
            sat(1.0 - abs(t + 0.5) * 1.5),
        );
        acc += vec4<f32>(s.rgb * w, s.a * w.g);
        cov_rgb += s.a * w;
    }
    // Normalize by the tap weights: each window sums to 2.
    let k = 0.5 * sat(-d / max(fg.params.y, 1e-3)) * sat(0.5 - d * px_per_pt);
    let src = acc * k;
    let cov = cov_rgb * k;
    return vec4<f32>(src.rgb + under.rgb * (1.0 - cov), src.a + under.a * (1.0 - src.a));
}
