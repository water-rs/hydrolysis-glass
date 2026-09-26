//! Element geometry and the signed-distance field, on the CPU.
//!
//! The GPU field pass in `shaders/field.wgsl` evaluates the same functions;
//! this module is the reference the verification tests measure the renders
//! against, and what the interaction layer uses to reason about gaps and
//! bridges.
//!
//! Conventions: distances in points, `d < 0` inside, `d = 0` on the
//! silhouette, `d > 0` outside; y grows downward.

/// Which curve a corner follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CornerCurve {
    /// A quarter circle: the classic rounded rectangle.
    Circular,
    /// A squircle-family corner: a quarter superellipse of exponent
    /// [`CONTINUOUS_EXPONENT`], evaluated radially.
    #[default]
    Continuous,
}

/// Superellipse exponent of a `continuous` corner. The specification only
/// pins the corner to "between 2 (circle) and ∞ (square)", bulging outward
/// of the circular arc by ≲0.5 px at the capture resolution; `2.02` puts the
/// bulge at ≈0.14 pt on a 40 pt radius (0.4 px at 3 px/pt). **[unknown;
/// chosen]**
pub const CONTINUOUS_EXPONENT: f32 = 2.02;

/// Deep-interior clamp of the field. **[logged]**
pub const NEGATIVE_SENTINEL: f32 = -10000.0;

/// Union smoothing of a standalone element's field, pt. **[logged]**
pub const STANDALONE_UNION_SMOOTHING: f32 = 8.0;

/// Union smoothing of a container's shared field, pt. **[logged]**
pub const CONTAINER_UNION_SMOOTHING: f32 = 30.0;

/// Fraction of the union smoothing used as the log-sum-exp temperature τ:
/// 30 pt of smoothing gives the τ ≈ 9 pt fitted to the container's fillets.
/// **[inferred]**
pub const SMOOTHING_TO_TAU: f32 = 0.3;

/// An axis-aligned rectangle in points.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Rect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
}

impl Rect {
    /// A rectangle from its origin and size.
    #[must_use]
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// Centre point.
    #[must_use]
    pub const fn center(&self) -> [f32; 2] {
        [self.w.mul_add(0.5, self.x), self.h.mul_add(0.5, self.y)]
    }

    /// `min(w, h)`: the size the recipes key on.
    #[must_use]
    pub const fn m(&self) -> f32 {
        self.w.min(self.h)
    }

    /// The rectangle scaled by `factor` about its centre.
    #[must_use]
    pub fn scaled_about_center(&self, factor: f32) -> Self {
        let [cx, cy] = self.center();
        let w = self.w * factor;
        let h = self.h * factor;
        Self::new(w.mul_add(-0.5, cx), h.mul_add(-0.5, cy), w, h)
    }

    /// The rectangle translated by `(dx, dy)`.
    #[must_use]
    pub fn translated(&self, dx: f32, dy: f32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.w, self.h)
    }
}

/// One element's silhouette: a box with per-corner radii, a corner curve and
/// an ovalization factor for the normals.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    /// Bounds in points.
    pub rect: Rect,
    /// Corner radii, `[top-left, top-right, bottom-right, bottom-left]`,
    /// clamped to `≤ min(w, h) / 2` when evaluated.
    pub radii: [f32; 4],
    /// The corner curve.
    pub curve: CornerCurve,
    /// `0` leaves the normal as the field gradient; `1` bends it fully onto
    /// the inscribed ellipse's normal. **[logged: 0 or 0.5]**
    pub ovalization: f32,
}

impl Shape {
    /// A rounded rectangle with one radius on every corner.
    #[must_use]
    pub const fn rounded_rect(rect: Rect, radius: f32, curve: CornerCurve) -> Self {
        Self {
            rect,
            radii: [radius; 4],
            curve,
            ovalization: 0.0,
        }
    }

    /// A capsule: radius `h / 2` with circular ends.
    #[must_use]
    pub const fn capsule(rect: Rect) -> Self {
        Self::rounded_rect(rect, rect.h * 0.5, CornerCurve::Circular)
    }

    /// A circle of the given centre and radius.
    #[must_use]
    pub const fn circle(cx: f32, cy: f32, radius: f32) -> Self {
        Self::rounded_rect(
            Rect::new(cx - radius, cy - radius, radius * 2.0, radius * 2.0),
            radius,
            CornerCurve::Circular,
        )
    }

    /// Sets the ovalization factor.
    #[must_use]
    pub const fn with_ovalization(mut self, ovalization: f32) -> Self {
        self.ovalization = ovalization;
        self
    }

    /// Sets the four corner radii individually.
    #[must_use]
    pub const fn with_radii(mut self, radii: [f32; 4]) -> Self {
        self.radii = radii;
        self
    }

    /// `min(w, h)` of the bounds.
    #[must_use]
    pub const fn m(&self) -> f32 {
        self.rect.m()
    }

    fn clamped_radius(&self, corner: usize) -> f32 {
        self.radii[corner].clamp(0.0, self.rect.m() * 0.5)
    }

    /// Signed distance from `p` to the silhouette.
    #[must_use]
    pub fn distance(&self, p: [f32; 2]) -> f32 {
        let [cx, cy] = self.rect.center();
        let hx = self.rect.w * 0.5;
        let hy = self.rect.h * 0.5;
        let lx = p[0] - cx;
        let ly = p[1] - cy;
        // Corner index by quadrant: 0 TL, 1 TR, 2 BR, 3 BL.
        let corner = match (lx >= 0.0, ly >= 0.0) {
            (false, false) => 0,
            (true, false) => 1,
            (true, true) => 2,
            (false, true) => 3,
        };
        let r = self.clamped_radius(corner);
        let qx = lx.abs() - (hx - r);
        let qy = ly.abs() - (hy - r);
        if qx > 0.0 && qy > 0.0 {
            let len = qx.hypot(qy);
            match self.curve {
                CornerCurve::Circular => len - r,
                CornerCurve::Continuous => {
                    if len <= f32::EPSILON || r <= f32::EPSILON {
                        return len - r;
                    }
                    let n = CONTINUOUS_EXPONENT;
                    let c = qx / len;
                    let s = qy / len;
                    // Radial reach of the superellipse |x/r|^n + |y/r|^n = 1
                    // along the direction (c, s).
                    let reach = r / (c.powf(n) + s.powf(n)).powf(1.0 / n);
                    len - reach
                }
            }
        } else {
            qx.max(qy) - r
        }
    }

    /// Gradient of the distance at `p` by central differences.
    #[must_use]
    pub fn gradient(&self, p: [f32; 2]) -> [f32; 2] {
        const H: f32 = 0.05;
        let dx = self.distance([p[0] + H, p[1]]) - self.distance([p[0] - H, p[1]]);
        let dy = self.distance([p[0], p[1] + H]) - self.distance([p[0], p[1] - H]);
        normalize([dx, dy])
    }

    /// Outward unit normal at `p`: the field gradient bent toward the
    /// inscribed ellipse's normal by `ovalization`.
    #[must_use]
    pub fn normal(&self, p: [f32; 2]) -> [f32; 2] {
        let g = self.gradient(p);
        if self.ovalization <= 0.0 {
            return g;
        }
        let e = ellipse_normal(self.rect, p);
        normalize([
            (e[0] - g[0]).mul_add(self.ovalization, g[0]),
            (e[1] - g[1]).mul_add(self.ovalization, g[1]),
        ])
    }
}

/// Normal of the ellipse inscribed in `rect`, evaluated radially at `p`.
#[must_use]
pub fn ellipse_normal(rect: Rect, p: [f32; 2]) -> [f32; 2] {
    let [cx, cy] = rect.center();
    let a = (rect.w * 0.5).max(f32::EPSILON);
    let b = (rect.h * 0.5).max(f32::EPSILON);
    normalize([(p[0] - cx) / (a * a), (p[1] - cy) / (b * b)])
}

fn normalize(v: [f32; 2]) -> [f32; 2] {
    let len = v[0].hypot(v[1]);
    if len <= 1e-12 {
        [0.0, -1.0]
    } else {
        [v[0] / len, v[1] / len]
    }
}

/// A field sample: fused distance, outward normal and owning member.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldSample {
    /// Fused signed distance, pt.
    pub distance: f32,
    /// Outward unit normal of the fused field.
    pub normal: [f32; 2],
    /// Index of the member with the smallest raw distance.
    pub owner: usize,
}

/// A shared field over a set of member shapes: the log-sum-exp smooth
/// minimum of the members' fields with ownership from the nearest member.
#[derive(Clone, Debug, PartialEq)]
pub struct UnionField<'a> {
    members: &'a [Shape],
    /// Union smoothing in pt (8 standalone, 30 container).
    smoothing: f32,
}

impl<'a> UnionField<'a> {
    /// A field over `members` with the given union smoothing.
    ///
    /// # Panics
    /// If `members` is empty.
    #[must_use]
    pub fn new(members: &'a [Shape], smoothing: f32) -> Self {
        assert!(!members.is_empty(), "a field needs at least one member");
        Self { members, smoothing }
    }

    /// The log-sum-exp temperature τ.
    #[must_use]
    pub fn tau(&self) -> f32 {
        (self.smoothing * SMOOTHING_TO_TAU).max(1e-3)
    }

    /// Fused distance at `p`, clamped below at the negative sentinel.
    #[must_use]
    pub fn distance(&self, p: [f32; 2]) -> f32 {
        self.distance_and_owner(p).0
    }

    fn distance_and_owner(&self, p: [f32; 2]) -> (f32, usize) {
        let tau = self.tau();
        let mut min_d = f32::INFINITY;
        let mut owner = 0;
        let mut raw = [0.0f32; 64];
        let count = self.members.len().min(raw.len());
        for (j, member) in self.members.iter().take(count).enumerate() {
            let d = member.distance(p);
            raw[j] = d;
            if d < min_d {
                min_d = d;
                owner = j;
            }
        }
        if count == 1 {
            return (min_d.max(NEGATIVE_SENTINEL), owner);
        }
        let sum: f32 = raw[..count].iter().map(|d| ((min_d - d) / tau).exp()).sum();
        (tau.mul_add(-sum.ln(), min_d).max(NEGATIVE_SENTINEL), owner)
    }

    /// Distance, normal and owner at `p`.
    #[must_use]
    pub fn sample(&self, p: [f32; 2]) -> FieldSample {
        const H: f32 = 0.05;
        let (distance, owner) = self.distance_and_owner(p);
        let gx = self.distance([p[0] + H, p[1]]) - self.distance([p[0] - H, p[1]]);
        let gy = self.distance([p[0], p[1] + H]) - self.distance([p[0], p[1] - H]);
        let g = normalize([gx, gy]);
        let member = self.members[owner];
        let normal = if member.ovalization > 0.0 {
            let e = ellipse_normal(member.rect, p);
            normalize([
                (e[0] - g[0]).mul_add(member.ovalization, g[0]),
                (e[1] - g[1]).mul_add(member.ovalization, g[1]),
            ])
        } else {
            g
        };
        FieldSample {
            distance,
            normal,
            owner,
        }
    }
}

/// The meniscus profile `μ(x; h) = 1 − sqrt(t·(2 − t))`, `t = sat(x / h)`:
/// 1 at `x = 0`, infinite-slope start, 0 at `x = h`. **[decoded]**
#[must_use]
pub fn meniscus(x: f32, h: f32) -> f32 {
    if h <= 0.0 {
        return if x <= 0.0 { 1.0 } else { 0.0 };
    }
    let t = (x / h).clamp(0.0, 1.0);
    1.0 - (t * (2.0 - t)).sqrt()
}

/// Antialiased coverage `sat(0.5 − d / fw)` for a field derivative of one
/// output pixel: `fw = 1 / px_per_pt`.
#[must_use]
pub fn coverage(d: f32, px_per_pt: f32) -> f32 {
    d.mul_add(-px_per_pt, 0.5).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circle_distance_is_radial() {
        let c = Shape::circle(0.0, 0.0, 30.0);
        assert!((c.distance([0.0, 0.0]) + 30.0).abs() < 1e-4);
        assert!(c.distance([30.0, 0.0]).abs() < 1e-4);
        assert!((c.distance([0.0, 40.0]) - 10.0).abs() < 1e-4);
        let n = c.normal([0.0, 30.0]);
        assert!(n[0].abs() < 1e-3 && (n[1] - 1.0).abs() < 1e-3);
    }

    #[test]
    fn rounded_rect_edges_and_corners() {
        let r = Shape::rounded_rect(
            Rect::new(0.0, 0.0, 300.0, 200.0),
            40.0,
            CornerCurve::Circular,
        );
        assert!(r.distance([150.0, 0.0]).abs() < 1e-4);
        assert!((r.distance([150.0, 100.0]) + 100.0).abs() < 1e-4);
        // Corner centre at (40, 40): the diagonal point at radius 40 is on the silhouette.
        let s = std::f32::consts::FRAC_1_SQRT_2 * 40.0;
        assert!(r.distance([40.0 - s, 40.0 - s]).abs() < 1e-3);
    }

    #[test]
    fn continuous_corner_bulges_slightly_outward() {
        let rect = Rect::new(0.0, 0.0, 300.0, 200.0);
        let circ = Shape::rounded_rect(rect, 40.0, CornerCurve::Circular);
        let cont = Shape::rounded_rect(rect, 40.0, CornerCurve::Continuous);
        let s = std::f32::consts::FRAC_1_SQRT_2 * 40.0;
        let p = [40.0 - s, 40.0 - s];
        let bulge = circ.distance(p) - cont.distance(p);
        assert!(bulge > 0.0 && bulge < 0.5 / 3.0, "bulge {bulge}");
        // Away from the corners the two fields agree exactly.
        assert!((circ.distance([150.0, 5.0]) - cont.distance([150.0, 5.0])).abs() < 1e-5);
    }

    #[test]
    fn ovalization_bends_normal_toward_ellipse() {
        let rect = Rect::new(0.0, 0.0, 300.0, 200.0);
        let flat = Shape::rounded_rect(rect, 40.0, CornerCurve::Continuous);
        let oval = flat.with_ovalization(0.5);
        // Mid top edge: gradient is straight up, ellipse normal is too.
        let n = oval.normal([150.0, 0.0]);
        assert!((n[1] + 1.0).abs() < 1e-3);
        // Near the top-left shoulder the ovalized normal gains an x component.
        let p = [60.0, 0.5];
        assert!(flat.normal(p)[0].abs() < 1e-2);
        assert!(oval.normal(p)[0] < -0.05);
    }

    #[test]
    fn union_keeps_16pt_gap_open_and_bridges_smoothly() {
        let circles = [
            Shape::circle(30.0, 30.0, 30.0),
            Shape::circle(106.0, 30.0, 30.0),
            Shape::circle(182.0, 30.0, 30.0),
        ];
        let field = UnionField::new(&circles, CONTAINER_UNION_SMOOTHING);
        assert!((field.tau() - 9.0).abs() < 1e-5);
        // Midpoint of the 16 pt gap: still outside (positive) but pulled in.
        let mid = field.distance([68.0, 30.0]);
        let raw = circles[0].distance([68.0, 30.0]);
        assert!(mid > 0.0, "gap must stay open: {mid}");
        assert!(
            mid < raw,
            "smooth union must pull the gap in: {mid} vs {raw}"
        );
        // Ownership follows the nearest member.
        assert_eq!(field.sample([100.0, 30.0]).owner, 1);
        // Deep inside a member the union equals that member's own field to within τ·ln(count) tails.
        let inside = field.distance([30.0, 30.0]);
        assert!((inside - circles[0].distance([30.0, 30.0])).abs() < 0.01);
    }

    #[test]
    fn meniscus_endpoints() {
        assert!((meniscus(0.0, 20.0) - 1.0).abs() < 1e-6);
        assert!(meniscus(20.0, 20.0).abs() < 1e-6);
        assert!(meniscus(40.0, 20.0).abs() < 1e-6);
        let mid = meniscus(10.0, 20.0);
        assert!((mid - (1.0 - 0.75f32.sqrt())).abs() < 1e-6);
    }

    #[test]
    fn coverage_is_one_pixel_wide() {
        assert!((coverage(-1.0, 3.0) - 1.0).abs() < 1e-6);
        assert!((coverage(0.0, 3.0) - 0.5).abs() < 1e-6);
        assert!(coverage(1.0, 3.0).abs() < 1e-6);
    }
}
