//! Recipes: the per-element parameter block and its size-scaling laws.
//!
//! Every value carries the specification's provenance tag in its doc
//! comment. `m = min(w, h)` in points is the only size input; the closed
//! forms hold exactly at every measured point.

/// Light or dark appearance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Appearance {
    /// Dark appearance.
    #[default]
    Dark,
    /// Light appearance.
    Light,
}

/// The style variant of an element.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Variant {
    /// Opaque frost: quarter-resolution capture, piecewise blur, face tint,
    /// bleed and a drop shadow.
    #[default]
    Regular,
    /// Nearly transparent lens: half-resolution capture, `blurR = 1`, no
    /// fill, shadow or bleed.
    Clear,
    /// The regular recipe plus the interactive shadow contribution and the
    /// tint sibling layers.
    Interactive,
}

/// A black point / white point / chroma scale colour matrix, optionally with
/// a folded-in premultiplied fill. **[decoded]**
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorMatrix {
    /// Black point `B`.
    pub black: f32,
    /// White point `W`.
    pub white: f32,
    /// Chroma scale `S`.
    pub saturation: f32,
    /// Premultiplied fill `[r, g, b, a]`, composited under the matrixed
    /// sample (destination-over), so it only shows where the sample is
    /// translucent.
    pub fill: [f32; 4],
}

impl ColorMatrix {
    /// A matrix with no fill.
    #[must_use]
    pub const fn new(black: f32, white: f32, saturation: f32) -> Self {
        Self {
            black,
            white,
            saturation,
            fill: [0.0; 4],
        }
    }

    /// Adds a fill of the given straight colour and alpha.
    #[must_use]
    pub const fn with_fill(mut self, rgb: [f32; 3], alpha: f32) -> Self {
        self.fill = [rgb[0] * alpha, rgb[1] * alpha, rgb[2] * alpha, alpha];
        self
    }

    /// Applies the matrix to an unpremultiplied encoded colour: luma is
    /// remapped onto `[B, W]`, chroma deviation scaled by `S`.
    #[must_use]
    pub fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        let y = luma(c);
        let base = y.mul_add(self.white - self.black, self.black);
        [
            self.saturation.mul_add(c[0] - y, base),
            self.saturation.mul_add(c[1] - y, base),
            self.saturation.mul_add(c[2] - y, base),
        ]
    }

    /// Applies the matrix to a premultiplied sample and folds in the fill,
    /// returning a premultiplied result. Mirrors `apply_matrix` in
    /// `shaders/material.wgsl`.
    #[must_use]
    pub fn apply_premultiplied(&self, c: [f32; 4]) -> [f32; 4] {
        let (rgb, a) = unpremultiply(c);
        let out = self.apply(rgb);
        let keep = 1.0 - a;
        [
            self.fill[0].mul_add(keep, out[0] * a),
            self.fill[1].mul_add(keep, out[1] * a),
            self.fill[2].mul_add(keep, out[2] * a),
            self.fill[3].mul_add(keep, a),
        ]
    }
}

/// Encoded-space luma weights. **[decoded]**
pub const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];

/// Alpha floor and channel-zeroing threshold of the unpremultiply step.
/// **[decoded, ε ≈ 1e-4]**
pub const UNPREMULTIPLY_EPSILON: f32 = 1e-4;

/// Luma of an encoded colour.
#[must_use]
pub fn luma(c: [f32; 3]) -> f32 {
    c[2].mul_add(LUMA[2], c[1].mul_add(LUMA[1], c[0] * LUMA[0]))
}

/// Unpremultiplies with the specification's ε handling.
#[must_use]
pub fn unpremultiply(c: [f32; 4]) -> ([f32; 3], f32) {
    let a = c[3].max(UNPREMULTIPLY_EPSILON);
    let f = |v: f32| {
        let s = v / a;
        if s.abs() < UNPREMULTIPLY_EPSILON {
            0.0
        } else {
            s
        }
    };
    ([f(c[0]), f(c[1]), f(c[2])], c[3])
}

/// A straight (unpremultiplied) tint colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tint {
    /// Straight RGB, encoded.
    pub rgb: [f32; 3],
    /// Declared alpha; the effective composited opacity is
    /// [`TINT_EFFECTIVE_ALPHA`] times this.
    pub alpha: f32,
}

/// Effective-to-declared tint alpha ratio. **[inferred]**
pub const TINT_EFFECTIVE_ALPHA: f32 = 0.4;

/// Parameters of the two-light edge highlight. **[logged]**
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Highlight {
    /// Curvature: the band loses this fraction of its weight by its inner edge.
    pub curvature: f32,
    /// Key light amount.
    pub key_amount: f32,
    /// Fill light amount.
    pub fill_amount: f32,
    /// Key light angle, radians.
    pub key_angle: f32,
    /// Fill light angle, radians.
    pub fill_angle: f32,
    /// Angular spread of both lights, radians. **[adaptive]**
    pub spread: f32,
    /// Key light colour, straight RGBA.
    pub key_color: [f32; 4],
    /// Fill light colour, straight RGBA.
    pub fill_color: [f32; 4],
}

/// Width of the highlight band, pt. **[decoded]**
pub const HIGHLIGHT_BAND: f32 = 1.33;

impl Highlight {
    /// The logged highlight for an element of short side `m`.
    #[must_use]
    pub fn for_size(m: f32) -> Self {
        Self {
            curvature: 0.7,
            key_amount: 0.5,
            fill_amount: 0.5,
            key_angle: -std::f32::consts::FRAC_PI_4,
            fill_angle: 3.0 * std::f32::consts::FRAC_PI_4,
            spread: highlight_spread(m),
            key_color: [1.0, 1.0, 1.0, 1.0],
            fill_color: [1.0, 1.0, 1.0, 1.0],
        }
    }
}

/// Highlight spread: π/2 for most elements, ≈2.755 rad on the 200 pt
/// element. Ramped linearly over `m ∈ [80, 200]` **[adaptive; the ramp
/// between the two logged values is chosen]**.
#[must_use]
pub fn highlight_spread(m: f32) -> f32 {
    let t = ((m - 80.0) / 120.0).clamp(0.0, 1.0);
    std::f32::consts::FRAC_PI_2 + t * (2.755 - std::f32::consts::FRAC_PI_2)
}

/// The full per-element parameter block of the background effect plus the
/// sibling layers (highlight, tint).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Recipe {
    /// Backdrop capture scale: 0.25 regular-class, 0.5 clear. **[logged]**
    pub capture_scale: f32,
    /// Capture luma-tracking flag (1 on the container only). **[logged]**
    pub luma_tracking: bool,
    /// 2×2 matrix applied to the field normal before displacing, row-major.
    pub disp_mat: [f32; 4],
    /// Inner lobe amount, pt (negative → magnify).
    pub inner_amt: f32,
    /// Inner lobe band height, pt.
    pub inner_h: f32,
    /// Outer lobe amount, pt (positive → inward).
    pub outer_amt: f32,
    /// Outer lobe band height, pt.
    pub outer_h: f32,
    /// Outer lobe distance window, pt.
    pub refr_thr: [f32; 2],
    /// Outer lobe mix weight cap.
    pub refr_op: f32,
    /// Base blur radius, capture texels.
    pub blur_r: f32,
    /// Depth breakpoints of the blur profile, pt.
    pub blur_dist: [f32; 5],
    /// Per-breakpoint blur weights.
    pub blur_op: [f32; 5],
    /// Face colour matrix and fill.
    pub face: ColorMatrix,
    /// Face-matrix mix weight.
    pub face_op: f32,
    /// Bleed lobe amount, pt.
    pub bleed_amt: f32,
    /// Bleed lobe band height, pt.
    pub bleed_h: f32,
    /// Bleed sample blur, capture texels.
    pub bleed_blur_r: f32,
    /// Bleed weight cap.
    pub bleed_op: f32,
    /// Bleed depth ramp `[full-at, zero-at]`... stored as logged `(dist0, dist1) = (1, 0)`:
    /// weight 1 at `d ≤ dist1`, 0 at `d ≥ dist0`.
    pub bleed_dist: [f32; 2],
    /// Bleed colour matrix.
    pub bleed_cm: ColorMatrix,
    /// Luma-gate polarity: `false` gates on dark surroundings, `true` on bright.
    pub bleed_darken: bool,
    /// Shadow shape offset, pt (y-down).
    pub shadow_offset: [f32; 2],
    /// Shadow lobe amount, pt.
    pub shadow_amt: f32,
    /// Shadow lobe band height, pt.
    pub shadow_h: f32,
    /// Shadow sample blur, capture texels.
    pub shadow_blur_r: f32,
    /// Shadow falloff reach divisor, pt.
    pub shadow_radius: f32,
    /// Shadow weight cap.
    pub shadow_op: f32,
    /// Backdrop-vs-fill mix inside the shadow colour (1 = backdrop).
    pub shadow_contrib: f32,
    /// Shadow colour matrix and fill.
    pub shadow_cm: ColorMatrix,
    /// SDR holding band, pt.
    pub sdr_dist: [f32; 2],
    /// SDR band strength.
    pub sdr_op: f32,
    /// SDR band tone: inside the band the premultiplied colour is pulled to
    /// `sdrWhite · sat(alpha)` — the fixed tone whose straight colour is
    /// `sdrWhite` at any coverage. **[logged: 1.0]**
    pub sdr_white: f32,
    /// Output luminance clamp; 9999 = inert.
    pub max_headroom: f32,
    /// Hue-preserving (`true`) or per-channel clamp.
    pub preserve_hue: bool,
    /// Final scalar on premultiplied RGB.
    pub edr_scale: f32,
    /// Positive field range, pt: beyond it the backdrop shows directly.
    pub positive_range: f32,
    /// Whether the material is enabled (else the interior is the blurred backdrop).
    pub material_enabled: bool,
    /// The edge highlight sibling.
    pub highlight: Highlight,
    /// Tint sibling layers, if any.
    pub tint: Option<Tint>,
}

impl Recipe {
    /// The recipe for a `variant` element of short side `m` in `appearance`.
    ///
    /// ```
    /// use hydrolysis_glass::material::recipe::{Appearance, Recipe, Variant};
    /// let r = Recipe::for_size(200.0, Variant::Regular, Appearance::Dark);
    /// assert_eq!(r.inner_amt, -60.0);
    /// assert_eq!(r.blur_r, 4.0);
    /// assert!((r.shadow_op - 0.25).abs() < 1e-3);
    /// ```
    #[must_use]
    pub fn for_size(m: f32, variant: Variant, appearance: Appearance) -> Self {
        let clear = variant == Variant::Clear;
        let light = appearance == Appearance::Light;
        let regular_class = !clear;

        let (inner_amt, inner_h) = if clear {
            (-m.min(60.0), (0.36 * m).min(20.0))
        } else {
            (-(0.8 * m).min(60.0), (0.25 * m).min(20.0))
        };

        let blur_dist4 = if clear { 0.0 } else { (0.2 * m).min(40.0) };
        let face = face_matrix(m, variant, appearance);

        // The 0.8·sat((m−56)/144) ramp gives 0.02 at m=60 where 0 was
        // logged; below the blur switch the bleed is off to hold the logged
        // points exactly.
        let bleed_op = if clear || m < BLUR_SWITCH_M {
            0.0
        } else if light {
            0.5 * ((m - 56.0) / 144.0).clamp(0.0, 1.0)
        } else {
            0.8 * ((m - 56.0) / 144.0).clamp(0.0, 1.0)
        };
        let bleed_cm = if clear {
            ColorMatrix::new(0.75, 1.0, 1.2)
        } else if light {
            ColorMatrix::new(0.9, 1.0, 1.2)
        } else {
            ColorMatrix::new(0.0, 0.5, 1.0)
        };

        let shadow_op = if clear {
            0.0
        } else if light {
            0.25f32.mul_add(((200.0 - m) / 160.0).clamp(0.0, 1.0), 0.25)
        } else {
            0.15f32.mul_add(((200.0 - m) / 160.0).clamp(0.0, 1.0), 0.25)
        };
        let shadow_cm = if clear {
            ColorMatrix::new(0.0, 1.0, 1.2).with_fill([0.0; 3], 0.1)
        } else if light {
            let fill_alpha = if m >= 80.0 { 0.12 } else { 0.3 };
            ColorMatrix::new(0.0, 1.0, 1.8).with_fill([0.0; 3], fill_alpha)
        } else {
            ColorMatrix::new(0.0, 0.5, 1.0)
        };
        let shadow_contrib = match variant {
            Variant::Interactive => 0.1667,
            Variant::Regular if m >= 200.0 => 1.0,
            _ => 0.0,
        };

        Self {
            capture_scale: if clear { 0.5 } else { 0.25 },
            luma_tracking: false,
            disp_mat: [1.0, 0.0, 0.0, 1.0],
            inner_amt,
            inner_h,
            outer_amt: 0.2 * m,
            outer_h: 0.125 * m,
            refr_thr: [-1.0, -2.0 / 3.0],
            refr_op: if clear { 0.0 } else { 0.3 },
            blur_r: blur_r(m, variant),
            blur_dist: [-(0.5 * m).min(100.0), -1.0, 0.0, 0.0, blur_dist4],
            blur_op: [1.0, 0.5, 0.5, 1.0, 1.0],
            face,
            face_op: 1.0,
            bleed_amt: if clear { 0.0 } else { 0.35 * m },
            bleed_h: if clear { 0.0 } else { 0.35 * m },
            bleed_blur_r: if regular_class { bleed_blur_r(m) } else { 0.0 },
            bleed_op,
            bleed_dist: [1.0, 0.0],
            bleed_cm,
            bleed_darken: clear,
            shadow_offset: [0.0, 8.0],
            shadow_amt: (0.625 * m).min(75.0),
            shadow_h: (0.4 * m).min(80.0),
            shadow_blur_r: shadow_blur_r(m),
            shadow_radius: 24.0,
            shadow_op,
            shadow_contrib,
            shadow_cm,
            sdr_dist: [-2.0 / 3.0, -1.0 / 3.0],
            sdr_op: sdr_op(m),
            sdr_white: 1.0,
            max_headroom: 9999.0,
            preserve_hue: true,
            edr_scale: 1.0,
            positive_range: if clear { 1.0 } else { positive_range(m) },
            material_enabled: true,
            highlight: Highlight::for_size(m),
            tint: None,
        }
    }

    /// The recipe for an element with the given bounds size.
    #[must_use]
    pub fn for_element(w: f32, h: f32, variant: Variant, appearance: Appearance) -> Self {
        Self::for_size(w.min(h), variant, appearance)
    }

    /// Adds the tint sibling layers.
    #[must_use]
    pub const fn with_tint(mut self, tint: Tint) -> Self {
        self.tint = Some(tint);
        self
    }

    /// Sets the capture luma-tracking flag (the container union used 1).
    #[must_use]
    pub const fn with_luma_tracking(mut self, on: bool) -> Self {
        self.luma_tracking = on;
        self
    }

    /// The recipe with the press response applied: `shadowOp × (1 + 1.2·press)`
    /// capped at 1, `shadowBlurR × (1 + 0.4·press)`. **[inferred]**
    #[must_use]
    pub fn pressed(mut self, press: f32) -> Self {
        let press = press.clamp(0.0, 1.0);
        self.shadow_op = (self.shadow_op * 1.2f32.mul_add(press, 1.0)).min(1.0);
        self.shadow_blur_r *= 0.4f32.mul_add(press, 1.0);
        self
    }

    /// The piecewise-linear blur weight `w(d)` of the depth profile.
    #[must_use]
    pub fn blur_weight(&self, d: f32) -> f32 {
        piecewise(&self.blur_dist, &self.blur_op, d)
    }

    /// Effective blur radius `blurR · w(d)`, capture texels.
    #[must_use]
    pub fn blur_radius(&self, d: f32) -> f32 {
        self.blur_r * self.blur_weight(d)
    }
}

/// Piecewise-linear interpolation through `(xs[i], ys[i])`, clamped at the
/// ends. Equal consecutive `xs` produce a step: the later value applies from
/// that `x` on.
#[must_use]
pub fn piecewise(xs: &[f32; 5], ys: &[f32; 5], x: f32) -> f32 {
    if x <= xs[0] {
        return ys[0];
    }
    for i in 0..4 {
        if x < xs[i + 1] {
            let span = xs[i + 1] - xs[i];
            if span <= 1e-6 {
                return ys[i + 1];
            }
            let t = (x - xs[i]) / span;
            return t.mul_add(ys[i + 1] - ys[i], ys[i]);
        }
    }
    ys[4]
}

/// `blurR` against `m`: flat 1 for clear; for regular a saturating ramp
/// through the measured points (1.333 at 34.3 and 40, 1.516 at 55.7,
/// 1.619 at 60, 2.095 at 80, 4.0 at 200) **[logged points; the curve between
/// them is inferred]**. Below 40 pt the value is held; between the points
/// the ramp is a monotone cubic through the table, which stays within the
/// ±10 % verification tolerance everywhere.
#[must_use]
pub fn blur_r(m: f32, variant: Variant) -> f32 {
    if variant == Variant::Clear {
        return 1.0;
    }
    const TABLE: [(f32, f32); 5] = [
        (40.0, 1.333),
        (55.7, 1.516),
        (60.0, 1.619),
        (80.0, 2.095),
        (200.0, 4.0),
    ];
    if m <= TABLE[0].0 {
        return TABLE[0].1;
    }
    if m >= TABLE[4].0 {
        return TABLE[4].1;
    }
    for w in TABLE.windows(2) {
        let (x0, y0) = w[0];
        let (x1, y1) = w[1];
        if m < x1 {
            let t = (m - x0) / (x1 - x0);
            let s = t * t * 2.0f32.mul_add(-t, 3.0);
            return (y1 - y0).mul_add(s, y0);
        }
    }
    TABLE[4].1
}

/// Size at which `bleedBlurR` and `shadowBlurR` switch on. The switch lies
/// between the m=60 (off) and m=80 (on) captures; treated as a step at 64 pt
/// per the specification's own assumption. **[inferred]**
pub const BLUR_SWITCH_M: f32 = 64.0;

/// `bleedBlurR`: `0.7·m` at and above the switch, else 0. **[logged]**
#[must_use]
pub fn bleed_blur_r(m: f32) -> f32 {
    if m >= BLUR_SWITCH_M { 0.7 * m } else { 0.0 }
}

/// `shadowBlurR`: 40 at and above the switch, else 0. **[logged]**
#[must_use]
pub fn shadow_blur_r(m: f32) -> f32 {
    if m >= BLUR_SWITCH_M { 40.0 } else { 0.0 }
}

/// `sdrOp ≈ 0.08 + 0.001·(m − 40)` clamped to `[0.08, 0.24]`. **[inferred]**
#[must_use]
pub fn sdr_op(m: f32) -> f32 {
    0.001f32.mul_add(m - 40.0, 0.08).clamp(0.08, 0.24)
}

/// Positive field range for regular-class elements: the logged values span
/// 34.5–39.5 pt (34.46 at m=200, 37.08 capsule, 37.6 container) with no
/// identified law; 37 pt is used at every size. **[unknown; chosen]**
#[must_use]
pub const fn positive_range(_m: f32) -> f32 {
    37.0
}

/// The adaptive face matrix per size, variant and appearance. Values are the
/// logged sets; between the logged sizes the nearer set applies (steps at
/// 48 and 70 pt) **[logged sets; step positions chosen]**.
#[must_use]
pub fn face_matrix(m: f32, variant: Variant, appearance: Appearance) -> ColorMatrix {
    if variant == Variant::Clear {
        return ColorMatrix::new(0.075, 1.15, 1.06);
    }
    match appearance {
        Appearance::Dark => {
            if m < 48.0 {
                ColorMatrix::new(0.225, 0.825, 1.0).with_fill([0.0; 3], 0.384)
            } else if m < 70.0 {
                ColorMatrix::new(0.25625, 0.85625, 1.0).with_fill([0.0; 3], 0.395)
            } else {
                ColorMatrix::new(0.2, 0.6, 1.0).with_fill([0.0; 3], 0.4)
            }
        }
        Appearance::Light => {
            if m < 48.0 {
                ColorMatrix::new(0.19375, 0.79375, 1.0).with_fill([1.0; 3], 0.578)
            } else if m < 70.0 {
                ColorMatrix::new(0.225, 0.825, 1.0).with_fill([1.0; 3], 0.555)
            } else {
                ColorMatrix::new(0.5, 1.03, 1.0).with_fill([1.0; 3], 0.4)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn size_laws_hit_logged_points() {
        let r40 = Recipe::for_size(40.0, Variant::Regular, Appearance::Dark);
        let r60 = Recipe::for_size(60.0, Variant::Regular, Appearance::Dark);
        let r80 = Recipe::for_size(80.0, Variant::Interactive, Appearance::Dark);
        let r200 = Recipe::for_size(200.0, Variant::Regular, Appearance::Dark);
        assert_eq!(
            [r40.inner_amt, r60.inner_amt, r80.inner_amt, r200.inner_amt],
            [-32.0, -48.0, -60.0, -60.0]
        );
        assert_eq!([r40.inner_h, r60.inner_h, r80.inner_h], [10.0, 15.0, 20.0]);
        assert_eq!(
            [r40.outer_amt, r80.outer_amt, r200.outer_amt],
            [8.0, 16.0, 40.0]
        );
        assert_eq!([r40.outer_h, r80.outer_h, r200.outer_h], [5.0, 10.0, 25.0]);
        assert_eq!([r40.blur_dist[0], r200.blur_dist[0]], [-20.0, -100.0]);
        assert_eq!(
            [r40.blur_dist[4], r80.blur_dist[4], r200.blur_dist[4]],
            [8.0, 16.0, 40.0]
        );
        assert!(close(r40.bleed_amt, 14.0, 1e-5) && close(r200.bleed_amt, 70.0, 1e-3));
        assert_eq!(
            [r60.bleed_blur_r, r80.bleed_blur_r, r200.bleed_blur_r],
            [0.0, 56.0, 140.0]
        );
        assert_eq!(
            [r40.shadow_amt, r80.shadow_amt, r200.shadow_amt],
            [25.0, 50.0, 75.0]
        );
        assert_eq!(
            [r40.shadow_h, r80.shadow_h, r200.shadow_h],
            [16.0, 32.0, 80.0]
        );
        assert_eq!(
            [r60.shadow_blur_r, r80.shadow_blur_r, r200.shadow_blur_r],
            [0.0, 40.0, 40.0]
        );
        assert!(close(r200.shadow_contrib, 1.0, 0.0) && close(r80.shadow_contrib, 0.1667, 1e-4));
        assert_eq!(r60.shadow_contrib, 0.0);
    }

    #[test]
    fn clear_laws() {
        let c40 = Recipe::for_size(40.0, Variant::Clear, Appearance::Light);
        let c60 = Recipe::for_size(60.0, Variant::Clear, Appearance::Dark);
        assert_eq!(c40.inner_amt, -40.0);
        assert!(close(c40.inner_h, 14.4, 1e-5));
        assert_eq!(c60.inner_amt, -60.0);
        assert_eq!(c60.inner_h, 20.0);
        assert_eq!(c40.capture_scale, 0.5);
        assert_eq!(c40.blur_r, 1.0);
        assert_eq!(c40.blur_dist[4], 0.0);
        assert_eq!(c40.refr_op, 0.0);
        assert_eq!(c40.shadow_op, 0.0);
        assert_eq!(c40.bleed_op, 0.0);
        assert_eq!(c40.positive_range, 1.0);
        assert_eq!(c40.face, ColorMatrix::new(0.075, 1.15, 1.06));
        assert_eq!(
            c40,
            Recipe::for_size(40.0, Variant::Clear, Appearance::Dark)
        );
    }

    #[test]
    fn opacity_ramps_match_logged_table() {
        for (m, dark, light, sdr, bd, bl) in [
            (40.0, 0.4, 0.5, 0.08, 0.0, 0.0),
            (55.7, 0.3897, 0.4821, 0.091, 0.0, 0.0),
            (60.0, 0.3839, 0.4732, 0.0971, 0.0, 0.0),
            (80.0, 0.3571, 0.4286, 0.1257, 0.133, 0.0833),
            (200.0, 0.25, 0.25, 0.24, 0.8, 0.5),
        ] {
            let d = Recipe::for_size(m, Variant::Regular, Appearance::Dark);
            let l = Recipe::for_size(m, Variant::Regular, Appearance::Light);
            assert!(
                close(d.shadow_op, dark, 0.01),
                "shadowOp dark m={m}: {}",
                d.shadow_op
            );
            assert!(
                close(l.shadow_op, light, 0.01),
                "shadowOp light m={m}: {}",
                l.shadow_op
            );
            assert!(close(d.sdr_op, sdr, 0.006), "sdrOp m={m}: {}", d.sdr_op);
            assert!(
                close(d.bleed_op, bd, 0.01),
                "bleedOp dark m={m}: {}",
                d.bleed_op
            );
            assert!(
                close(l.bleed_op, bl, 0.01),
                "bleedOp light m={m}: {}",
                l.bleed_op
            );
        }
    }

    #[test]
    fn blur_r_matches_table_within_ten_percent() {
        for (m, want) in [
            (34.3, 1.333),
            (40.0, 1.333),
            (55.7, 1.516),
            (60.0, 1.619),
            (80.0, 2.095),
            (200.0, 4.0),
        ] {
            let got = blur_r(m, Variant::Regular);
            assert!(close(got, want, want * 0.1), "blurR m={m}: {got} vs {want}");
            assert!(
                close(got, want, 1e-3),
                "blurR must hold exactly at logged points: {got} vs {want}"
            );
        }
        assert_eq!(blur_r(120.0, Variant::Clear), 1.0);
    }

    #[test]
    fn blur_profile_dips_at_silhouette() {
        let r = Recipe::for_size(200.0, Variant::Regular, Appearance::Dark);
        assert!(close(r.blur_weight(-100.0), 1.0, 1e-6));
        assert!(close(r.blur_weight(-1.0), 0.5, 1e-6));
        assert!(close(r.blur_weight(-0.5), 0.5, 1e-6));
        assert!(close(r.blur_weight(0.0), 1.0, 1e-6));
        assert!(close(r.blur_weight(50.0), 1.0, 1e-6));
        assert!(close(r.blur_weight(-50.5), 0.75, 1e-6));
        assert!(close(r.blur_radius(-1.0), 2.0, 1e-6));
    }

    #[test]
    fn color_matrix_remaps_luma() {
        let face = ColorMatrix::new(0.2, 0.6, 1.0);
        let out = face.apply([0.85, 0.85, 0.85]);
        assert!(close(out[0], 0.54, 1e-5) && close(out[1], 0.54, 1e-5));
        let sat = ColorMatrix::new(0.0, 1.0, 1.2).apply([1.0, 0.0, 0.0]);
        assert!(close(sat[0], 1.2f32.mul_add(1.0 - 0.2126, 0.2126), 1e-5));
        let (rgb, a) = unpremultiply([0.5, 0.25, 0.0, 0.5]);
        assert!(close(rgb[0], 1.0, 1e-6) && close(rgb[1], 0.5, 1e-6) && a == 0.5);
        assert_eq!(unpremultiply([0.0, 0.0, 0.0, 0.0]).0, [0.0; 3]);
    }

    #[test]
    fn press_scales_shadow() {
        let r = Recipe::for_size(200.0, Variant::Regular, Appearance::Dark).pressed(1.0);
        assert!(close(r.shadow_op, 0.55, 1e-5));
        assert!(close(r.shadow_blur_r, 56.0, 1e-5));
        let small = Recipe::for_size(40.0, Variant::Regular, Appearance::Light).pressed(1.0);
        assert!(close(small.shadow_op, 1.0, 1e-6), "capped at 1");
    }

    #[test]
    fn light_mode_keeps_distances() {
        let d = Recipe::for_size(200.0, Variant::Regular, Appearance::Dark);
        let l = Recipe::for_size(200.0, Variant::Regular, Appearance::Light);
        assert_eq!(
            (d.inner_amt, d.blur_r, d.shadow_amt, d.bleed_amt),
            (l.inner_amt, l.blur_r, l.shadow_amt, l.bleed_amt)
        );
        assert_eq!((l.face.black, l.face.white), (0.5, 1.03));
        assert_eq!(l.bleed_cm, ColorMatrix::new(0.9, 1.0, 1.2));
        assert_eq!(l.shadow_cm.saturation, 1.8);
        assert!(close(l.shadow_cm.fill[3], 0.12, 1e-6));
    }
}
