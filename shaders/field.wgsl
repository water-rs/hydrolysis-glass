// The field pass: rasterizes one group's shared signed-distance field.
//
// Output per pixel: (distance pt, normal.x, normal.y, owner index).
// The fused distance is the quadratic smooth minimum over the members;
// the owner is the member with the smallest raw distance; the normal is the
// fused field's gradient bent toward the owner's inscribed-ellipse normal by
// its recipe's ellipse bend.

struct FieldElement {
    rect: vec4<f32>,     // x, y, w, h in pt
    radii: vec4<f32>,    // TL, TR, BR, BL
    params: vec4<f32>,   // curve (0 circular, 1 continuous), ellipse bend, -, -
}

struct FieldUniforms {
    // scene width/height in pt, px per pt, union blend distance k (pt)
    scene: vec4<f32>,
    // member count, continuous exponent, region origin px
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

// Whether member `j` at distance `d` sorts after `(after_d, after_j)`:
// ascending distance, exact ties in member order.
fn sorts_after(d: f32, j: i32, after_d: f32, after_j: i32) -> bool {
    return d > after_d || (d == after_d && j > after_j);
}

fn fused_distance(p: vec2<f32>) -> Fused {
    let count = i32(field.misc.x);
    let k = field.scene.w;
    // Every member, folded in ascending distance order with the quadratic
    // smooth minimum, as `UnionField` does: the fold is not associative,
    // so the order is part of the field. Each step selects the next member
    // in that order, so no member count bounds the pass.
    var out: Fused;
    var fused = 0.0;
    var last_d = -3.4e38;
    var last_j = -1;
    for (var step = 0; step < count; step += 1) {
        var next_d = 3.4e38;
        var next_j = count;
        for (var j = 0; j < count; j += 1) {
            let d = shape_distance(members[j], p);
            if sorts_after(d, j, last_d, last_j) && sorts_after(next_d, next_j, d, j) {
                next_d = d;
                next_j = j;
            }
        }
        if step == 0 {
            fused = next_d;
            out.owner = u32(next_j);
        } else {
            let h = max(k - abs(fused - next_d), 0.0) / k;
            fused = min(fused, next_d) - h * h * k * 0.25;
        }
        last_d = next_d;
        last_j = next_j;
    }
    out.distance = max(fused, NEGATIVE_SENTINEL);
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
    // The field texture covers the group's bounds, not the scene: local
    // texels map onto scene pt through the region origin.
    let p = (in.position.xy + field.misc.zw) / px_per_pt;
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
    let bend = owner.params.y;
    var n = g;
    if bend > 0.0 {
        n = normalize(mix(g, ellipse_normal(owner, p), bend));
    }
    return vec4<f32>(f.distance, n.x, n.y, f32(f.owner));
}
