// The field pass: rasterizes one group's shared signed-distance field.
//
// Output per pixel: (distance pt, normal.x, normal.y, owner index).
// The fused distance is the log-sum-exp smooth minimum over the members;
// the owner is the member with the smallest raw distance; the normal is the
// fused field's gradient bent toward the owner's inscribed-ellipse normal by
// its ovalization.

struct FieldElement {
    rect: vec4<f32>,     // x, y, w, h in pt
    radii: vec4<f32>,    // TL, TR, BR, BL
    params: vec4<f32>,   // curve (0 circular, 1 continuous), ovalization, -, -
}

struct FieldUniforms {
    // scene width/height in pt, px per pt, tau (pt)
    scene: vec4<f32>,
    // member count, continuous exponent, -, -
    misc: vec4<f32>,
}

@group(0) @binding(0) var<uniform> field: FieldUniforms;
@group(0) @binding(1) var<storage, read> members: array<FieldElement>;

const NEGATIVE_SENTINEL: f32 = -10000.0;

fn shape_distance(e: FieldElement, p: vec2<f32>) -> f32 {
    let half = e.rect.zw * 0.5;
    let c = e.rect.xy + half;
    let l = p - c;
    var corner = 0u;
    if l.x >= 0.0 && l.y < 0.0 { corner = 1u; }
    if l.x >= 0.0 && l.y >= 0.0 { corner = 2u; }
    if l.x < 0.0 && l.y >= 0.0 { corner = 3u; }
    let r = clamp(e.radii[corner], 0.0, min(half.x, half.y));
    let q = abs(l) - (half - vec2<f32>(r));
    if q.x > 0.0 && q.y > 0.0 {
        let len = length(q);
        if e.params.x > 0.5 && len > 1e-6 && r > 1e-6 {
            let n = field.misc.y;
            let dir = q / len;
            let reach = r / pow(pow(dir.x, n) + pow(dir.y, n), 1.0 / n);
            return len - reach;
        }
        return len - r;
    }
    return max(q.x, q.y) - r;
}

struct Fused {
    distance: f32,
    owner: u32,
}

fn fused_distance(p: vec2<f32>) -> Fused {
    let count = u32(field.misc.x);
    let tau = max(field.scene.w, 1e-3);
    var min_d = 1e30;
    var owner = 0u;
    for (var j = 0u; j < count; j += 1u) {
        let d = shape_distance(members[j], p);
        if d < min_d {
            min_d = d;
            owner = j;
        }
    }
    var out: Fused;
    out.owner = owner;
    if count <= 1u {
        out.distance = max(min_d, NEGATIVE_SENTINEL);
        return out;
    }
    var sum = 0.0;
    for (var j = 0u; j < count; j += 1u) {
        let d = shape_distance(members[j], p);
        sum += exp((min_d - d) / tau);
    }
    out.distance = max(min_d - tau * log(sum), NEGATIVE_SENTINEL);
    return out;
}

fn ellipse_normal(e: FieldElement, p: vec2<f32>) -> vec2<f32> {
    let half = max(e.rect.zw * 0.5, vec2<f32>(1e-6));
    let c = e.rect.xy + half;
    let v = (p - c) / (half * half);
    let len = length(v);
    if len < 1e-12 {
        return vec2<f32>(0.0, -1.0);
    }
    return v / len;
}

@fragment
fn field_fragment(in: FullscreenOut) -> @location(0) vec4<f32> {
    let px_per_pt = field.scene.z;
    let p = in.position.xy / px_per_pt;
    let f = fused_distance(p);
    let h = 0.05;
    let gx = fused_distance(p + vec2<f32>(h, 0.0)).distance - fused_distance(p - vec2<f32>(h, 0.0)).distance;
    let gy = fused_distance(p + vec2<f32>(0.0, h)).distance - fused_distance(p - vec2<f32>(0.0, h)).distance;
    var g = vec2<f32>(gx, gy);
    if length(g) < 1e-12 {
        g = vec2<f32>(0.0, -1.0);
    } else {
        g = normalize(g);
    }
    let owner = members[f.owner];
    let oval = owner.params.y;
    var n = g;
    if oval > 0.0 {
        n = normalize(mix(g, ellipse_normal(owner, p), oval));
    }
    return vec4<f32>(f.distance, n.x, n.y, f32(f.owner));
}
