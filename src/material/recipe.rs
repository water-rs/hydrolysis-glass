//! Recipes: the per-element material parameters and their size-scaling
//! laws.
//!
//! Every value carries the specification's evidence tag in its doc comment
//! (`docs/specification.md`): measured, fitted or chosen. The size driver is `m = min(w, h)` of the element's bounds in points; a recipe
//! never depends on the backdrop's content. Every size law is evaluated at
//! `m` clamped to the family's measured range **\[chosen\]**.

use core::f32::consts::{FRAC_PI_2, PI};

/// Light or dark appearance. It changes the regular family only; the clear
/// family is identical in both. **[measured]**
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Appearance {
    /// Dark appearance.
    #[default]
    Dark,
    /// Light appearance.
    Light,
}

/// The material family of an element.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Family {
    /// A frosted surface that hides detail behind it and changes with the
    /// appearance.
    #[default]
    Regular,
    /// A nearly transparent lens that keeps the content behind it legible;
    /// identical in both appearances.
    Clear,
}

/// The user translucency setting `s ∈ [0, 1]`, default 0.5. It changes
/// both families (spec §5.4) and is chosen by the user: it does not follow
/// the content behind the glass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Translucency(f32);

impl Translucency {
    /// The default setting, 0.5. **[measured]**
    pub const DEFAULT: Self = Self(0.5);

    /// The setting.
    ///
    /// # Panics
    /// If `value` is not finite or lies outside `[0, 1]`.
    #[must_use]
    pub fn new(value: f32) -> Self {
        assert!(
            (0.0..=1.0).contains(&value),
            "translucency {value} outside [0, 1]"
        );
        Self(value)
    }

    /// The setting's value.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl Default for Translucency {
    fn default() -> Self {
        Self(0.5)
    }
}

/// Measured range of the regular family, pt — size laws are clamped to it.
/// **[measured]**
pub const REGULAR_MEASURED_RANGE: [f32; 2] = [34.33, 200.33];
/// Measured range of the clear family, pt — size laws are clamped to it.
/// **[measured]**
pub const CLEAR_MEASURED_RANGE: [f32; 2] = [40.0, 80.0];

/// Encoded-space luma weights of §4.6 — the BT.709 weight set of the
/// model's YCbCr construction. **[fitted, part of the §4.6 form]**
pub const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
/// Luma weights of the §4.5 luminance limit. **[fitted, part of the §4.5
/// form]**
pub const LUMA_LIMIT: [f32; 3] = [0.2126, 0.7153, 0.0722];
/// Luma weights of the §4.7 ambient gate. **[fitted, part of the §4.7
/// form]**
pub const LUMA_GATE: [f32; 3] = [0.2125, 0.7153, 0.0721];

/// Alpha floor and channel-zeroing threshold of the unpremultiply step.
/// **[chosen: the spec's unpremultiply has no ε]**
pub const UNPREMULTIPLY_EPSILON: f32 = 1e-4;

/// Effective-to-declared tint alpha ratio: the composite strength of the
/// §4.13 tint layers is open; the layers composite at 0.4 × declared.
/// **[chosen]**
pub const TINT_EFFECTIVE_ALPHA: f32 = 0.4;

/// Width of the §4.12 highlight band, pt. **[chosen: keeps the current
/// implementation's form; the rim it produces is ≈1 pt, as measured]**
pub const HIGHLIGHT_BAND: f32 = 1.33;

/// Luma of an encoded colour (§4.6 weights).
#[must_use]
pub fn luma(c: [f32; 3]) -> f32 {
    c[2].mul_add(LUMA[2], c[1].mul_add(LUMA[1], c[0] * LUMA[0]))
}

/// Unpremultiplies with the ε handling: channels whose straight value is
/// smaller than ε zero out.
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

/// §3: the quarter-circle lobe a displaced sample follows,
/// `shift · (1 − sqrt(t·(2 − t)))` with `t = sat(−d / depth)`.
/// **[chosen]**
#[must_use]
pub fn lobe(d: f32, depth: f32, shift: f32) -> f32 {
    if depth <= 0.0 {
        return if d <= 0.0 { shift } else { 0.0 };
    }
    let t = (-d / depth).clamp(0.0, 1.0);
    shift * (1.0 - (t * (2.0 - t)).sqrt())
}

/// §3: the mip level a blur radius of `b` capture texels selects,
/// `max(0, log2(b < 2 ? 1 + b/2 : b))`. **[chosen]**
#[must_use]
pub fn level(b: f32) -> f32 {
    (if b < 2.0 { 1.0 + b / 2.0 } else { b }).log2().max(0.0)
}

/// §3: the smooth step weighting the shadow terms; runs from 1 at
/// `x = −2` to 0 at `x = 2`. **[fitted]**
#[must_use]
pub fn falloff(x: f32) -> f32 {
    let x2 = x * x;
    x.mul_add(
        x2.mul_add(
            x2.mul_add(0.002_954_5, -0.034_454).mul_add(x2, 0.168_21),
            -0.5605,
        ),
        0.5,
    )
}

/// Piecewise-linear interpolation through the four `(xs[i], ys[i])`
/// breakpoints of §4.3.
///
/// `ys[0]` below `xs[0]`, `ys[3]` above `xs[3]`; equal consecutive `xs`
/// produce a step with the later value applying from that `x` on.
/// **[chosen]**
#[must_use]
pub fn piecewise(xs: &[f32; 4], ys: &[f32; 4], x: f32) -> f32 {
    if x <= xs[0] {
        return ys[0];
    }
    for i in 0..3 {
        if x < xs[i + 1] {
            let span = xs[i + 1] - xs[i];
            if span <= 1e-6 {
                return ys[i + 1];
            }
            let t = (x - xs[i]) / span;
            return t.mul_add(ys[i + 1] - ys[i], ys[i]);
        }
    }
    ys[3]
}

/// §5.4: linear interpolation of a translucency row — `v0 → v05` over
/// `s ∈ [0, 0.5]`, `v05 → v1` over `s ∈ [0.5, 1]`. Rows the spec marks
/// *linear* interpolate this way; rows it leaves open between samples use
/// the same interpolation. **[chosen for the open intervals]**
#[must_use]
fn translucency_row(v0: f32, v05: f32, v1: f32, s: f32) -> f32 {
    if s <= 0.5 {
        (v05 - v0).mul_add(s * 2.0, v0)
    } else {
        (v1 - v05).mul_add(2.0f32.mul_add(s, -1.0), v05)
    }
}

/// §5.1 size-tabled values with measured points at `m ≤ 60`, `m = 80` and
/// `m = 200.33`: constant at `m ≤ 60`, linear from 60 to 80 and from 80 to
/// 200.33. **[chosen]**
#[must_use]
fn size_table(m: f32, v60: f32, v80: f32, v200: f32) -> f32 {
    if m <= 60.0 {
        v60
    } else if m <= 80.0 {
        (v80 - v60).mul_add((m - 60.0) / 20.0, v60)
    } else {
        (v200 - v80).mul_add((m - 80.0) / 120.33, v80)
    }
}

/// §5.1 laws that switch on between the measured sizes: 0 at `m ≤ 60`,
/// linear from the `m = 60` value to the `m = 80` value between 60 and 80,
/// the fitted law at `m ≥ 80`. **[chosen]**
#[must_use]
fn switching_law(m: f32, fit: impl Fn(f32) -> f32) -> f32 {
    if m <= 60.0 {
        0.0
    } else if m < 80.0 {
        fit(80.0) * (m - 60.0) / 20.0
    } else {
        fit(m)
    }
}

/// Returns `sat((m − 56)/144)` — §5.1's ramp at `m ≥ 80`, unit amplitude;
/// callers scale it by their measured coefficient. **[chosen: the
/// interpolating curve is one of many through the measured points]**
fn ramp_144(m: f32) -> f32 {
    ((m - 56.0) / 144.0).clamp(0.0, 1.0)
}

/// §5.1: the dark appearance's luminance limit — the §5.1 size table
/// (0.6 at `m ≤ 60`, 0.5 at `m = 80`, 0.35 at `m = 200.33`) composed with
/// the §5.4 translucency rows evaluated at each knot. **[measured rows;
/// the joint size × translucency composition is chosen]**
#[must_use]
fn luminance_limit(m: f32, s: f32) -> f32 {
    let l80 = translucency_row(0.54, 0.5, 0.5, s);
    let l200 = translucency_row(0.45, 0.35, 0.35, s);
    if m <= 60.0 {
        0.6
    } else if m <= 80.0 {
        (l80 - 0.6).mul_add((m - 60.0) / 20.0, 0.6)
    } else {
        (l200 - l80).mul_add((m - 80.0) / 120.33, l80)
    }
}

/// The §4.6 grade: a luma floor and ceiling and a chroma scale, applied
/// through the YCbCr construction. **[fitted]**
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grade {
    /// Luma floor `F`: the luma a black input maps to.
    pub floor: f32,
    /// Luma ceiling `K`: the luma a white input maps to.
    pub ceiling: f32,
    /// Chroma scale `S`.
    pub chroma: f32,
}

impl Grade {
    /// A grade from its three constants.
    #[must_use]
    pub const fn new(floor: f32, ceiling: f32, chroma: f32) -> Self {
        Self {
            floor,
            ceiling,
            chroma,
        }
    }

    /// Applies the grade to an unpremultiplied encoded colour through the
    /// §4.6 YCbCr construction: `Y' = (K − F)·Y + F`,
    /// `chroma' = S·(chroma − 0.5) + 0.5` on both `Cb` and `Cr`, then the
    /// inverse BT.709 transform. Mirrors `apply_grade` in
    /// `shaders/material.wgsl`.
    #[must_use]
    pub fn apply(&self, col: [f32; 3]) -> [f32; 3] {
        let lum = luma(col);
        let cb = (col[2] - lum).mul_add(1.0 / 1.8556, 0.5);
        let cr = (col[0] - lum).mul_add(1.0 / 1.5748, 0.5);
        let y2 = lum.mul_add(self.ceiling - self.floor, self.floor);
        let cb2 = self.chroma.mul_add(cb - 0.5, 0.5);
        let cr2 = self.chroma.mul_add(cr - 0.5, 0.5);
        let red = 1.5748f32.mul_add(cr2 - 0.5, y2);
        let blu = 1.8556f32.mul_add(cb2 - 0.5, y2);
        let green = 0.0722f32.mul_add(-blu, 0.2126f32.mul_add(-red, y2)) * (1.0 / 0.7152);
        [red, green, blu]
    }

    /// Applies the grade to a premultiplied sample: unpremultiply, apply,
    /// repremultiply by the sample's own alpha.
    #[must_use]
    pub fn apply_premultiplied(&self, c: [f32; 4]) -> [f32; 4] {
        let (s, a) = unpremultiply(c);
        let o = self.apply(s);
        [o[0] * a, o[1] * a, o[2] * a, a]
    }
}

/// §4.6's grade composite on an unpremultiplied colour:
/// `mix(C, (1 − wash.a)·graded + wash.a·wash.rgb, grade mix)`.
/// **[fitted]**
#[must_use]
pub fn grade_composite(
    grade: &Grade,
    wash: [f32; 3],
    wash_alpha: f32,
    mix: f32,
    c: [f32; 3],
) -> [f32; 3] {
    let graded = grade.apply(c);
    let washed = [
        wash_alpha.mul_add(wash[0] - graded[0], graded[0]),
        wash_alpha.mul_add(wash[1] - graded[1], graded[1]),
        wash_alpha.mul_add(wash[2] - graded[2], graded[2]),
    ];
    [
        mix.mul_add(washed[0] - c[0], c[0]),
        mix.mul_add(washed[1] - c[1], c[1]),
        mix.mul_add(washed[2] - c[2], c[2]),
    ]
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

/// Parameters of the §4.12 two-light rim highlight.
///
/// The bearing θ points a light along `(sin θ, −cos θ)` in y-down space: 0
/// lights the top edge, π the bottom. The band width, knee and composite
/// keep the current implementation's forms **[chosen]**, and the glow
/// term is a second band of the same two-light form at the measured glow
/// multipliers **[chosen]**.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Highlight {
    /// Inner fade: the band loses this fraction of its weight by its inner
    /// edge. **[measured: 0.75]**
    pub inner_fade: f32,
    /// First light's gain. **[measured: 0.5]**
    pub first_gain: f32,
    /// Second light's gain. **[measured: 0.5]**
    pub second_gain: f32,
    /// Soft-knee denominator of both lights `q / (1 + knee·(1 − q))`.
    /// **[chosen: keeps the current implementation's form]**
    pub knee: f32,
    /// First light's bearing, rad. **[measured: 0]**
    pub first_bearing: f32,
    /// Second light's bearing, rad. **[measured: π]**
    pub second_bearing: f32,
    /// Angular aperture of both lights, rad. **[measured size table: 4π/9 at
    /// m ≤ 80, π/2 at m = 200.33; the ramp between is chosen]**
    pub aperture: f32,
    /// Glow gain multiplier on the light gains. **[measured: 0.15]**
    pub glow_gain: f32,
    /// Glow width multiplier on the band width. **[measured: 8]**
    pub glow_width: f32,
    /// Glow aperture multiplier on the light aperture. **[measured: 0.65]**
    pub glow_aperture: f32,
    /// First light's colour, straight RGBA. **[measured: white @ 1]**
    pub first_colour: [f32; 4],
    /// Second light's colour, straight RGBA. **[measured: white @ 1]**
    pub second_colour: [f32; 4],
}

impl Highlight {
    /// The highlight for an element of short side `m` (already clamped to
    /// the family's measured range).
    #[must_use]
    pub fn for_size(m: f32) -> Self {
        Self {
            inner_fade: 0.75,
            first_gain: 0.5,
            second_gain: 0.5,
            knee: 0.5,
            first_bearing: 0.0,
            second_bearing: PI,
            aperture: size_table(m, 4.0 * PI / 9.0, 4.0 * PI / 9.0, FRAC_PI_2),
            glow_gain: 0.15,
            glow_width: 8.0,
            glow_aperture: 0.65,
            first_colour: [1.0; 4],
            second_colour: [1.0; 4],
        }
    }
}

/// The full per-element recipe of the glass body plus the sibling layers
/// (highlight, tint). Carries every parameter of spec §4
/// and §5; interaction does not change it (§5.5).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Recipe {
    /// §4.1/§5.4: backdrop capture scale, relative to the device pixel
    /// grid: the capture is `ceil(region_pt · px_per_pt · capture_scale)`
    /// texels per axis, so one capture texel is `region_pt / capture_size`
    /// pt per axis — at most `1 / (capture_scale · px_per_pt)` pt.
    /// **[measured rows; the m-split at 140 and the translucency
    /// interpolation are chosen. The device-pixel base is chosen: §4.1 does
    /// not state it, and it keeps the 8 pt grid countable through the clear
    /// card (§7, Frost) where a point base erases it]**
    pub capture_scale: f32,
    /// §4.3: base blur radius, capture texels. **[measured: 5 regular; the
    /// clear family's translucency row is linear]**
    pub blur_radius: f32,
    /// §4.3: blur depths `d0..d3`, pt. **[fitted / measured: (−0.5·m, −1, 0, 0)]**
    pub blur_depths: [f32; 4],
    /// §4.3: blur gains `g0..g3`. **[measured; the m-law of g0 is
    /// chosen; the clear family's translucency ramp is chosen]**
    pub blur_gains: [f32; 4],
    /// §4.2: primary sample's lobe shift, pt (negative → magnify).
    /// **[fitted: −min(0.5·m, 60) regular, −0.65·m clear]**
    pub primary_shift: f32,
    /// §4.2: primary sample's lobe depth, pt.
    /// **[fitted: min(0.25·m, 20) regular, min(0.36·m, 20) clear]**
    pub primary_depth: f32,
    /// §4.2: rim sample's lobe shift, pt (positive → inward).
    /// **[fitted: max(16, 0.25·m) regular, 0.2·m clear]**
    pub rim_shift: f32,
    /// §4.2: rim sample's lobe depth, pt.
    /// **[fitted: max(16, 0.2·m) regular, 0.125·m clear]**
    pub rim_depth: f32,
    /// §4.2: rim ramp `(r0, r1)`, pt. **[measured: (−1, 0)]**
    pub rim_ramp: [f32; 2],
    /// §4.2: rim mix — the rim sample's weight past the ramp. **[measured:
    /// 0.6 regular, 0 clear]**
    pub rim_mix: f32,
    /// §4.5: luminance limit `L`, applied through `1 − L`.
    /// **[measured size × translucency rows; the joint composition is
    /// chosen]**
    pub luminance_limit: f32,
    /// §4.6: grade. **[measured]**
    pub grade: Grade,
    /// §4.6: wash colour, straight RGB. **[measured translucency rows; the
    /// between-sample colours are chosen]**
    pub wash: [f32; 3],
    /// §4.6: wash alpha. **[measured translucency row, linear]**
    pub wash_alpha: f32,
    /// §4.6: grade mix. **[measured: 1]**
    pub grade_mix: f32,
    /// §4.7: ambient sample's lobe shift, pt. **[fitted: 0.35·m regular, 0 clear]**
    pub ambient_shift: f32,
    /// §4.7: ambient sample's lobe depth, pt. **[fitted: 0.35·m regular, 0 clear]**
    pub ambient_depth: f32,
    /// §4.7: ambient sample's blur radius, capture texels.
    /// **[measured / fitted: 0 at m ≤ 60, 0.35·m at m ≥ 80; the ramp between
    /// is chosen]**
    pub ambient_blur: f32,
    /// §4.7: ambient strength cap. **[measured size law; the ramp between 60
    /// and 80 is chosen]**
    pub ambient_strength: f32,
    /// §4.7: ambient ramp `(e0, e1)`, pt. **[measured: (1, 0)]**
    pub ambient_ramp: [f32; 2],
    /// §4.7: ambient grade. **[measured]**
    pub ambient_grade: Grade,
    /// §4.7: ambient gate polarity: `true` gates on `Yg`, weighting the
    /// ambient sample toward bright pixels (light appearance and the clear family);
    /// `false` gates on `1 − Yg`, toward dark ones (dark appearance).
    /// **[fitted]**
    pub ambient_favours_light: bool,
    /// §4.4: haze radius, capture texels; the taps' level and
    /// offset use `level(radius)`. **[measured: 8 regular; the clear
    /// family's translucency ramp is chosen]**
    pub haze_radius: f32,
    /// §4.4: haze weight of `min(C, F)`. **[measured translucency row,
    /// linear, dark regular]**
    pub haze_min_weight: f32,
    /// §4.4: haze weight of `max(C, F)`. **[measured translucency row,
    /// linear, light regular]**
    pub haze_max_weight: f32,
    /// §4.4: haze mix toward `F`. **[measured translucency row,
    /// linear]**
    pub haze_mix: f32,
    /// §4.8: contour shade density. **[measured: 0.06 regular, 0 clear]**
    pub contour_shade_density: f32,
    /// §4.8: contour shade shift, pt. **[measured: 8; its direction is open —
    /// taken downward (+y) like the cast shadow's, chosen]**
    pub contour_shade_shift: f32,
    /// §4.8: contour shade softness, pt. **[measured: 5]**
    pub contour_shade_softness: f32,
    /// §4.8: contour shade width, pt. **[measured: 4]**
    pub contour_shade_width: f32,
    /// §4.8: contour shade clip: 1 limits the shade to the glass coverage.
    /// **[measured: 1 regular, 0 clear]**
    pub contour_shade_clip: f32,
    /// §4.9: border correction strength. **[measured: 0.4]**
    pub border_strength: f32,
    /// §4.9: border lobe bearing, rad. **[measured: π/2]**
    pub border_bearing: f32,
    /// §4.9: border band width, pt. **[measured: 0.533]**
    pub border_band_width: f32,
    /// §4.9: border band offset, pt. **[measured: −0.533]**
    pub border_band_offset: f32,
    /// §4.9: border angular aperture, rad — the SDR-output value.
    /// **[measured: 75° dark and clear, 106° light on SDR output (96° on HDR
    /// output); using the SDR-output value is chosen]**
    pub border_aperture: f32,
    /// §4.9: border colour bias. **[measured: −0.3]**
    pub border_colour_bias: f32,
    /// §4.10: cast shadow shift, pt (y-down). **[measured: (0, 8)
    /// regular; not in the clear table — chosen, and inert since the
    /// clear cast density is 0]**
    pub cast_shift: [f32; 2],
    /// §4.10: cast shadow falloff length (the falloff's divisor), pt.
    /// **[measured size table; interpolation chosen]**
    pub cast_falloff: f32,
    /// §4.10: cast shadow density cap. **[measured size table; interpolation
    /// chosen]**
    pub cast_density: f32,
    /// §4.10: cast shadow gain — a per-family factor on the shadow weight.
    /// **[fitted: 0.3 regular; measured: 0.1 clear, inert since the clear
    /// cast density is 0]**
    pub cast_gain: f32,
    /// §4.11: channel ceiling, per channel.
    /// **[measured: 1.308 dark, 1.070 light, 1.192 clear]**
    pub channel_ceiling: f32,
    /// §4.11: exterior span, pt — the distance outside the silhouette over
    /// which the element's field is evaluated. **[measured size table;
    /// interpolation chosen]**
    pub exterior_span: f32,
    /// §2: ellipse bend — bends the normal in the corners toward the
    /// inscribed ellipse's. **[measured: 0.5 on the regular
    /// elements at m ≥ 80, 0 elsewhere; the size rule is chosen]**
    pub ellipse_bend: f32,
    /// §4.12: the two-light rim highlight sibling.
    pub highlight: Highlight,
    /// §4.13: tint sibling layers, if any.
    pub tint: Option<Tint>,
}

impl Recipe {
    /// The recipe for a `family` element of short side `m` in `appearance`
    /// at the translucency setting `translucency`. `m` is clamped to the
    /// family's measured range first. **[chosen clamp]**
    ///
    /// ```
    /// use hydrolysis_glass::material::recipe::{Appearance, Family, Recipe, Translucency};
    /// let r = Recipe::for_size(200.33, Family::Regular, Appearance::Dark, Translucency::default());
    /// assert_eq!(r.primary_shift, -60.0);
    /// assert_eq!(r.blur_radius, 5.0);
    /// assert!((r.cast_density - 0.6).abs() < 1e-3);
    /// ```
    #[must_use]
    pub fn for_size(
        m: f32,
        family: Family,
        appearance: Appearance,
        translucency: Translucency,
    ) -> Self {
        let s = translucency.get();
        let range = match family {
            Family::Regular => REGULAR_MEASURED_RANGE,
            Family::Clear => CLEAR_MEASURED_RANGE,
        };
        let m = m.clamp(range[0], range[1]);
        match family {
            Family::Regular => Self::regular(m, appearance, s),
            Family::Clear => Self::clear(m, s),
        }
    }

    /// The recipe for an element with the given bounds size.
    #[must_use]
    pub fn for_element(
        w: f32,
        h: f32,
        family: Family,
        appearance: Appearance,
        translucency: Translucency,
    ) -> Self {
        Self::for_size(w.min(h), family, appearance, translucency)
    }

    /// Adds the §4.13 tint sibling layers.
    #[must_use]
    pub const fn with_tint(mut self, tint: Tint) -> Self {
        self.tint = Some(tint);
        self
    }

    /// §4.3: the piecewise blur gain `g(d)` of the depth profile.
    #[must_use]
    pub fn blur_gain(&self, d: f32) -> f32 {
        piecewise(&self.blur_depths, &self.blur_gains, d)
    }

    /// §4.3: the effective blur radius at depth `d`, capture texels.
    #[must_use]
    pub fn blur_radius_at(&self, d: f32) -> f32 {
        self.blur_radius * self.blur_gain(d)
    }

    /// §5.1: the regular family at clamped `m`, appearance and setting.
    fn regular(m: f32, appearance: Appearance, s: f32) -> Self {
        let dark = appearance == Appearance::Dark;
        let blur_gain0 = switching_law(m, |m| 0.8 * ramp_144(m));
        let ambient_strength = if dark {
            switching_law(m, |m| 0.8 * ramp_144(m))
        } else {
            switching_law(m, |m| 0.5 * ramp_144(m))
        };
        let (grade, wash, wash_alpha) = if dark {
            (
                Grade::new(0.125, 1.125, 1.3),
                [0.125 * 2.0f32.mul_add(s, -1.0).abs(); 3],
                (s - 0.5).max(0.0),
            )
        } else {
            (
                Grade::new(0.4, 1.03, 1.2),
                [1.0; 3],
                translucency_row(0.0, 0.2, 0.5, s),
            )
        };
        let haze_dominant = (0.45f32.mul_add(s, 0.675)).min(0.9);
        Self {
            capture_scale: if m < 140.0 {
                translucency_row(0.5, 0.5, 0.125, s)
            } else {
                translucency_row(0.333, 0.25, 0.125, s)
            },
            blur_radius: 5.0,
            blur_depths: [-0.5 * m, -1.0, 0.0, 0.0],
            blur_gains: [blur_gain0, 0.5 * blur_gain0, 0.5, 1.0],
            primary_shift: -(0.5 * m).min(60.0),
            primary_depth: (0.25 * m).min(20.0),
            rim_shift: 16.0f32.max(0.25 * m),
            rim_depth: 16.0f32.max(0.2 * m),
            rim_ramp: [-1.0, 0.0],
            rim_mix: 0.6,
            luminance_limit: if dark { luminance_limit(m, s) } else { 1.0 },
            grade,
            wash,
            wash_alpha,
            grade_mix: 1.0,
            ambient_shift: 0.35 * m,
            ambient_depth: 0.35 * m,
            ambient_blur: switching_law(m, |m| 0.35 * m),
            ambient_strength,
            ambient_ramp: [1.0, 0.0],
            ambient_grade: if dark {
                Grade::new(0.125, 0.5, 1.0)
            } else {
                Grade::new(0.9, 1.0, 1.2)
            },
            ambient_favours_light: !dark,
            haze_radius: 8.0,
            haze_min_weight: if dark { haze_dominant } else { 0.0 },
            haze_max_weight: if dark { 0.0 } else { haze_dominant },
            haze_mix: s,
            contour_shade_density: 0.06,
            contour_shade_shift: 8.0,
            contour_shade_softness: 5.0,
            contour_shade_width: 4.0,
            contour_shade_clip: 1.0,
            border_strength: 0.4,
            border_bearing: FRAC_PI_2,
            border_band_width: 0.533,
            border_band_offset: -0.533,
            border_aperture: if dark {
                75.0f32.to_radians()
            } else {
                106.0f32.to_radians()
            },
            border_colour_bias: -0.3,
            cast_shift: [0.0, 8.0],
            cast_falloff: size_table(m, 4.0, 7.33, 24.0),
            cast_density: if dark {
                size_table(m, 0.04, 0.133, 0.6)
            } else {
                size_table(m, 0.04, 0.1, 0.4)
            },
            cast_gain: 0.3,
            channel_ceiling: if dark { 1.308 } else { 1.070 },
            exterior_span: if dark {
                size_table(m, 8.89, 9.11, 39.83)
            } else {
                size_table(m, 8.89, 8.89, 37.90)
            },
            ellipse_bend: if m >= 80.0 { 0.5 } else { 0.0 },
            highlight: Highlight::for_size(m),
            tint: None,
        }
    }

    /// §5.2: the clear family at clamped `m` — identical in both
    /// appearances. **[measured]**
    fn clear(m: f32, s: f32) -> Self {
        // The blur gains' translucency row is not marked linear;
        // the gains ramp with the radius's own ramp — saturating at
        // s = 0.5 — which reproduces all three measured rows. **[chosen]**
        let w = (2.0 * s).min(1.0);
        Self {
            capture_scale: 0.5,
            blur_radius: translucency_row(0.0, 0.75, 0.75, s),
            blur_depths: [-0.5 * m, -1.0, 0.0, 0.0],
            blur_gains: [w, 0.5 * w, 0.5 * w, w],
            primary_shift: -0.65 * m,
            primary_depth: (0.36 * m).min(20.0),
            rim_shift: 0.2 * m,
            rim_depth: 0.125 * m,
            rim_ramp: [-1.0, 0.0],
            rim_mix: 0.0,
            luminance_limit: 1.0,
            grade: Grade::new(0.125, 1.08, 1.06),
            wash: [1.0; 3],
            wash_alpha: 0.0,
            grade_mix: 1.0,
            ambient_shift: 0.0,
            ambient_depth: 0.0,
            ambient_blur: 0.0,
            ambient_strength: 0.0,
            ambient_ramp: [1.0, 0.0],
            ambient_grade: Grade::new(0.75, 1.0, 1.2),
            ambient_favours_light: true,
            haze_radius: 8.0 * translucency_row(0.0, 0.0, 1.0, s),
            haze_min_weight: 0.0,
            haze_max_weight: 0.0,
            haze_mix: translucency_row(0.0, 0.0, 1.0, s),
            contour_shade_density: 0.0,
            contour_shade_shift: 0.0,
            contour_shade_softness: 0.0,
            contour_shade_width: 0.0,
            contour_shade_clip: 0.0,
            border_strength: 0.4,
            border_bearing: FRAC_PI_2,
            border_band_width: 0.533,
            border_band_offset: -0.533,
            border_aperture: 75.0f32.to_radians(),
            border_colour_bias: -0.3,
            cast_shift: [0.0, 8.0],
            cast_falloff: 0.0,
            cast_density: 0.0,
            cast_gain: 0.1,
            channel_ceiling: 1.192,
            exterior_span: 1.533,
            ellipse_bend: 0.0,
            highlight: Highlight::for_size(m),
            tint: None,
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    reason = "tests assert the specification's measured values directly"
)]
mod tests {
    use super::*;

    const T: Translucency = Translucency(0.5);

    fn assert_close(a: &[f32], b: &[f32], tol: f32) {
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b) {
            assert!((x - y).abs() <= tol, "{x} vs {y}");
        }
    }

    fn regular(m: f32, appearance: Appearance) -> Recipe {
        Recipe::for_size(m, Family::Regular, appearance, T)
    }

    fn regular_s(m: f32, s: f32) -> Recipe {
        Recipe::for_size(m, Family::Regular, Appearance::Dark, Translucency::new(s))
    }

    fn clear_s(m: f32, s: f32) -> Recipe {
        Recipe::for_size(m, Family::Clear, Appearance::Dark, Translucency::new(s))
    }

    /// The measured regular columns at the default setting:
    /// `#1, #3, #4, #5, #6, #7/#8, #9` in order.
    fn regular_columns(appearance: Appearance) -> [Recipe; 7] {
        [
            regular(200.33, appearance),
            regular(80.0, appearance),
            regular(55.67, appearance),
            regular(34.33, appearance),
            regular(60.0, appearance),
            regular(40.0, appearance),
            regular(60.0, appearance),
        ]
    }

    #[test]
    fn regular_size_laws() {
        let c = regular_columns(Appearance::Dark);
        // Capture scale at s = 0.5.
        assert_close(
            &c.iter().map(|r| r.capture_scale).collect::<Vec<_>>(),
            &[0.25, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5],
            1e-6,
        );
        assert!(c.iter().all(|r| r.blur_radius == 5.0));
        // Blur depths (−0.5·m, −1, 0, 0).
        assert_eq!(
            c.iter().map(|r| r.blur_depths[0]).collect::<Vec<_>>(),
            [
                -0.5 * 200.33,
                -40.0,
                -0.5 * 55.67,
                -0.5 * 34.33,
                -30.0,
                -20.0,
                -30.0
            ]
        );
        assert!(c.iter().all(|r| r.blur_depths[1..] == [-1.0, 0.0, 0.0]));
        // Blur gains: g0 measured at 0.8 / 0.133 / 0…, g1 = g0/2.
        assert!((c[0].blur_gains[0] - 0.8).abs() < 1e-3);
        assert!((c[1].blur_gains[0] - 0.133).abs() < 1e-3);
        for r in &c[2..] {
            assert_eq!(r.blur_gains[0], 0.0);
        }
        for r in &c {
            assert_eq!(r.blur_gains[1], 0.5 * r.blur_gains[0]);
            assert_eq!(r.blur_gains[2..], [0.5, 1.0]);
        }
        // Primary lobe.
        assert_eq!(
            c.iter().map(|r| r.primary_shift).collect::<Vec<_>>(),
            [
                -60.0,
                -40.0,
                -0.5 * 55.67,
                -0.5 * 34.33,
                -30.0,
                -20.0,
                -30.0
            ]
        );
        assert_eq!(
            c.iter().map(|r| r.primary_depth).collect::<Vec<_>>(),
            [20.0, 20.0, 0.25 * 55.67, 0.25 * 34.33, 15.0, 10.0, 15.0]
        );
        // Rim lobe.
        assert_eq!(
            c.iter().map(|r| r.rim_shift).collect::<Vec<_>>(),
            [0.25 * 200.33, 20.0, 16.0, 16.0, 16.0, 16.0, 16.0]
        );
        assert_eq!(
            c.iter().map(|r| r.rim_depth).collect::<Vec<_>>(),
            [0.2 * 200.33, 16.0, 16.0, 16.0, 16.0, 16.0, 16.0]
        );
        // Refraction.
        assert!(
            c.iter()
                .all(|r| { r.rim_ramp == [-1.0, 0.0] && r.rim_mix == 0.6 })
        );
    }

    #[test]
    fn regular_grade_and_wash() {
        for (r, dark) in [
            (regular_columns(Appearance::Dark), true),
            (regular_columns(Appearance::Light), false),
        ]
        .iter()
        .flat_map(|(rs, d)| rs.iter().map(move |r| (r, *d)))
        {
            let (b, w, s) = (r.grade.floor, r.grade.ceiling, r.grade.chroma);
            if dark {
                assert_eq!([b, w, s], [0.125, 1.125, 1.3]);
                assert_eq!(r.wash, [0.0; 3]);
                assert_eq!(r.wash_alpha, 0.0);
            } else {
                assert_eq!([b, w, s], [0.4, 1.03, 1.2]);
                assert_eq!(r.wash, [1.0; 3]);
                assert_eq!(r.wash_alpha, 0.2);
            }
            assert_eq!(r.grade_mix, 1.0);
        }
    }

    #[test]
    fn regular_luminance_limit() {
        let d = regular_columns(Appearance::Dark);
        assert!((d[0].luminance_limit - 0.35).abs() < 1e-3);
        assert!((d[1].luminance_limit - 0.5).abs() < 1e-3);
        for r in &d[2..] {
            assert!((r.luminance_limit - 0.6).abs() < 1e-3);
        }
        assert!(
            regular_columns(Appearance::Light)
                .iter()
                .all(|r| r.luminance_limit == 1.0)
        );
    }

    #[test]
    fn regular_ambient() {
        let m = [200.33f32, 80.0, 55.67, 34.33, 60.0, 40.0, 60.0];
        let d = regular_columns(Appearance::Dark);
        let l = regular_columns(Appearance::Light);
        for (i, r) in d.iter().enumerate() {
            assert!(0.35f32.mul_add(-m[i], r.ambient_shift).abs() < 1e-3);
            assert_eq!(r.ambient_shift, r.ambient_depth);
            assert_eq!(r.ambient_ramp, [1.0, 0.0]);
        }
        assert!((d[0].ambient_blur - 70.12).abs() < 1e-2);
        assert!((d[1].ambient_blur - 28.0).abs() < 1e-3);
        for r in &d[2..] {
            assert_eq!(r.ambient_blur, 0.0);
        }
        assert!((d[0].ambient_strength - 0.8).abs() < 1e-3);
        assert!((d[1].ambient_strength - 0.133).abs() < 1e-3);
        assert!((l[0].ambient_strength - 0.5).abs() < 1e-3);
        assert!((l[1].ambient_strength - 0.083).abs() < 1e-3);
        for r in d[2..].iter().chain(&l[2..]) {
            assert_eq!(r.ambient_strength, 0.0);
        }
        for r in &d {
            assert_eq!(r.ambient_grade, Grade::new(0.125, 0.5, 1.0));
            assert!(!r.ambient_favours_light);
        }
        for r in &l {
            assert_eq!(r.ambient_grade, Grade::new(0.9, 1.0, 1.2));
            assert!(r.ambient_favours_light);
        }
    }

    #[test]
    fn regular_haze() {
        for r in &regular_columns(Appearance::Dark) {
            assert_eq!(r.haze_radius, 8.0);
            assert_eq!(r.haze_min_weight, 0.9);
            assert_eq!(r.haze_max_weight, 0.0);
            assert_eq!(r.haze_mix, 0.5);
        }
        for r in &regular_columns(Appearance::Light) {
            assert_eq!(r.haze_min_weight, 0.0);
            assert_eq!(r.haze_max_weight, 0.9);
            assert_eq!(r.haze_mix, 0.5);
        }
    }

    #[test]
    fn regular_contour_border_cast() {
        for appearance in [Appearance::Dark, Appearance::Light] {
            for r in &regular_columns(appearance) {
                assert_eq!(r.contour_shade_density, 0.06);
                assert_eq!(r.contour_shade_shift, 8.0);
                assert_eq!(r.contour_shade_softness, 5.0);
                assert_eq!(r.contour_shade_width, 4.0);
                assert_eq!(r.contour_shade_clip, 1.0);
                assert_eq!(r.border_strength, 0.4);
                assert_eq!(r.border_bearing, FRAC_PI_2);
                assert_eq!(r.border_band_width, 0.533);
                assert_eq!(r.border_band_offset, -0.533);
                assert_eq!(r.border_colour_bias, -0.3);
                assert_eq!(r.cast_shift, [0.0, 8.0]);
                assert_eq!(r.cast_gain, 0.3);
            }
        }
        let d = regular_columns(Appearance::Dark);
        let l = regular_columns(Appearance::Light);
        assert!((d[0].border_aperture - 75.0f32.to_radians()).abs() < 1e-6);
        assert!((l[0].border_aperture - 106.0f32.to_radians()).abs() < 1e-6);
        assert_close(
            &d.iter().map(|r| r.cast_falloff).collect::<Vec<_>>(),
            &[24.0, 7.33, 4.0, 4.0, 4.0, 4.0, 4.0],
            1e-3,
        );
        assert!((d[0].cast_density - 0.6).abs() < 1e-3);
        assert!((d[1].cast_density - 0.133).abs() < 1e-3);
        assert!((l[0].cast_density - 0.4).abs() < 1e-3);
        assert!((l[1].cast_density - 0.1).abs() < 1e-3);
        for r in d[2..].iter().chain(&l[2..]) {
            assert!((r.cast_density - 0.04).abs() < 1e-3);
        }
    }

    #[test]
    fn regular_ceiling_span_bend_highlight() {
        let d = regular_columns(Appearance::Dark);
        let l = regular_columns(Appearance::Light);
        assert!(d.iter().all(|r| r.channel_ceiling == 1.308));
        assert!(l.iter().all(|r| r.channel_ceiling == 1.070));
        assert!((d[0].exterior_span - 39.83).abs() < 1e-2);
        assert!((l[0].exterior_span - 37.90).abs() < 1e-2);
        assert!((d[1].exterior_span - 9.11).abs() < 1e-2);
        assert!((l[1].exterior_span - 8.89).abs() < 1e-2);
        for r in d[2..].iter().chain(&l[2..]) {
            assert!((r.exterior_span - 8.89).abs() < 1e-2);
        }
        // Ellipse bend 0.5 on #1 and #3, 0 elsewhere.
        assert_eq!(d[0].ellipse_bend, 0.5);
        assert_eq!(d[1].ellipse_bend, 0.5);
        for r in &d[2..] {
            assert_eq!(r.ellipse_bend, 0.0);
        }
        // Highlight: shared constants plus the measured aperture.
        for r in &d {
            let h = &r.highlight;
            assert_eq!(h.first_bearing, 0.0);
            assert_eq!(h.second_bearing, PI);
            assert_eq!(h.first_gain, 0.5);
            assert_eq!(h.second_gain, 0.5);
            assert_eq!(h.inner_fade, 0.75);
            assert_eq!(h.first_colour, [1.0; 4]);
            assert_eq!(h.second_colour, [1.0; 4]);
            assert_eq!(h.glow_gain, 0.15);
            assert_eq!(h.glow_width, 8.0);
            assert_eq!(h.glow_aperture, 0.65);
        }
        assert!((d[0].highlight.aperture - FRAC_PI_2).abs() < 1e-6);
        for r in &d[1..] {
            assert!((r.highlight.aperture - 4.0 * PI / 9.0).abs() < 1e-6);
        }
    }

    #[test]
    fn clear_columns() {
        let c = [clear_s(80.0, 0.5), clear_s(60.0, 0.5), clear_s(40.0, 0.5)];
        assert!(c.iter().all(|r| r.capture_scale == 0.5));
        assert!(c.iter().all(|r| r.blur_radius == 0.75));
        assert_eq!(
            c.iter().map(|r| r.blur_depths[0]).collect::<Vec<_>>(),
            [-40.0, -30.0, -20.0]
        );
        assert!(c.iter().all(|r| r.blur_gains == [1.0, 0.5, 0.5, 1.0]));
        assert_eq!(
            c.iter().map(|r| r.primary_shift).collect::<Vec<_>>(),
            [-52.0, -39.0, -26.0]
        );
        assert_close(
            &c.iter().map(|r| r.primary_depth).collect::<Vec<_>>(),
            &[20.0, 20.0, 14.4],
            1e-4,
        );
        for (r, (oa, oh)) in c.iter().zip([(16.0, 10.0), (12.0, 7.5), (8.0, 5.0)]) {
            assert!((r.rim_shift - oa).abs() < 1e-4);
            assert!((r.rim_depth - oh).abs() < 1e-4);
        }
        for r in &c {
            assert_eq!(r.rim_ramp, [-1.0, 0.0]);
            assert_eq!(r.rim_mix, 0.0);
            assert_eq!(r.grade, Grade::new(0.125, 1.08, 1.06));
            assert_eq!(r.wash, [1.0; 3]);
            assert_eq!(r.wash_alpha, 0.0);
            assert_eq!(r.grade_mix, 1.0);
            assert_eq!(r.luminance_limit, 1.0);
            assert_eq!(r.ambient_shift, 0.0);
            assert_eq!(r.ambient_depth, 0.0);
            assert_eq!(r.ambient_blur, 0.0);
            assert_eq!(r.ambient_strength, 0.0);
            assert_eq!(r.ambient_grade, Grade::new(0.75, 1.0, 1.2));
            assert!(r.ambient_favours_light);
            assert_eq!(r.haze_radius, 0.0);
            assert_eq!(r.haze_min_weight, 0.0);
            assert_eq!(r.haze_max_weight, 0.0);
            assert_eq!(r.haze_mix, 0.0);
            assert_eq!(r.contour_shade_density, 0.0);
            assert_eq!(r.contour_shade_clip, 0.0);
            assert_eq!(r.cast_density, 0.0);
            assert_eq!(r.cast_falloff, 0.0);
            assert_eq!(r.cast_gain, 0.1);
            assert_eq!(r.border_strength, 0.4);
            assert!((r.border_aperture - 75.0f32.to_radians()).abs() < 1e-6);
            assert_eq!(r.channel_ceiling, 1.192);
            assert_eq!(r.exterior_span, 1.533);
            assert_eq!(r.ellipse_bend, 0.0);
            assert!((r.highlight.aperture - 4.0 * PI / 9.0).abs() < 1e-6);
        }
        // Clear is identical in both appearances.
        for m in [80.0, 60.0, 40.0] {
            assert_eq!(
                Recipe::for_size(m, Family::Clear, Appearance::Light, T),
                Recipe::for_size(m, Family::Clear, Appearance::Dark, T),
            );
        }
    }

    #[test]
    fn translucency_table() {
        // Regular wash, dark: grey 0.125@0 → black@0 → grey 0.125@0.5.
        let f = |s| {
            let r = regular_s(200.33, s);
            (r.wash, r.wash_alpha)
        };
        let (rgb0, a0) = f(0.0);
        assert_eq!(rgb0, [0.125; 3]);
        assert_eq!(a0, 0.0);
        let (rgb1, a1) = f(0.5);
        assert_eq!(rgb1, [0.0; 3]);
        assert_eq!(a1, 0.0);
        let (rgb2, a2) = f(1.0);
        assert_eq!(rgb2, [0.125; 3]);
        assert_eq!(a2, 0.5);
        // Regular wash, light: white@0 → 0.2 → 0.5.
        let light = |s| {
            Recipe::for_size(
                200.33,
                Family::Regular,
                Appearance::Light,
                Translucency::new(s),
            )
            .wash_alpha
        };
        assert_close(
            &[light(0.0), light(0.5), light(1.0)],
            &[0.0, 0.2, 0.5],
            1e-6,
        );
        // Capture scale: element 1 and the m ≤ 80 row.
        let cap = |m, s| regular_s(m, s).capture_scale;
        assert_close(
            &[
                cap(200.33, 0.0),
                cap(200.33, 0.5),
                cap(200.33, 1.0),
                cap(80.0, 0.0),
                cap(80.0, 0.5),
                cap(80.0, 1.0),
            ],
            &[0.333, 0.25, 0.125, 0.5, 0.5, 0.125],
            1e-3,
        );
        // Haze min weight (dark) / max weight (light), and mix.
        let min_weight = |s| regular_s(200.33, s).haze_min_weight;
        assert_close(
            &[min_weight(0.0), min_weight(0.5), min_weight(1.0)],
            &[0.675, 0.9, 0.9],
            1e-6,
        );
        let max_weight = |s| {
            Recipe::for_size(
                200.33,
                Family::Regular,
                Appearance::Light,
                Translucency::new(s),
            )
            .haze_max_weight
        };
        assert_close(
            &[max_weight(0.0), max_weight(0.5), max_weight(1.0)],
            &[0.675, 0.9, 0.9],
            1e-6,
        );
        let normal = |s| regular_s(200.33, s).haze_mix;
        assert_close(
            &[normal(0.0), normal(0.5), normal(1.0)],
            &[0.0, 0.5, 1.0],
            1e-6,
        );
        // Luminance limits, dark: element 1, element 3, m ≤ 60.
        let lim = |m, s| regular_s(m, s).luminance_limit;
        assert_close(
            &[lim(200.33, 0.0), lim(200.33, 0.5), lim(200.33, 1.0)],
            &[0.45, 0.35, 0.35],
            1e-3,
        );
        assert_close(
            &[lim(80.0, 0.0), lim(80.0, 0.5), lim(80.0, 1.0)],
            &[0.54, 0.5, 0.5],
            1e-3,
        );
        assert_close(
            &[lim(60.0, 0.0), lim(60.0, 0.5), lim(60.0, 1.0)],
            &[0.6, 0.6, 0.6],
            1e-6,
        );
        clear_translucency();
    }

    fn clear_translucency() {
        // Clear: blur radius, gains, haze radius/mix.
        assert_close(
            &[
                clear_s(80.0, 0.0).blur_radius,
                clear_s(80.0, 0.5).blur_radius,
                clear_s(80.0, 1.0).blur_radius,
            ],
            &[0.0, 0.75, 0.75],
            1e-6,
        );
        assert_eq!(clear_s(80.0, 0.0).blur_gains, [0.0; 4]);
        assert_eq!(clear_s(80.0, 0.5).blur_gains, [1.0, 0.5, 0.5, 1.0]);
        assert_eq!(clear_s(80.0, 1.0).blur_gains, [1.0, 0.5, 0.5, 1.0]);
        assert_close(
            &[
                clear_s(80.0, 0.0).haze_radius,
                clear_s(80.0, 0.5).haze_radius,
                clear_s(80.0, 1.0).haze_radius,
            ],
            &[0.0, 0.0, 8.0],
            1e-6,
        );
        assert_close(
            &[
                clear_s(80.0, 0.0).haze_mix,
                clear_s(80.0, 0.5).haze_mix,
                clear_s(80.0, 1.0).haze_mix,
            ],
            &[0.0, 0.0, 1.0],
            1e-6,
        );
        assert_eq!(clear_s(80.0, 1.0).wash_alpha, 0.0);
    }

    #[test]
    fn translucency_midpoints_lie_on_the_linear_rows() {
        // The observations at 0.25 and 0.75.
        let normal = |s| regular_s(200.33, s).haze_mix;
        assert!((normal(0.25) - 0.25).abs() < 1e-6);
        assert!((normal(0.75) - 0.75).abs() < 1e-6);
        let min_weight = |s| regular_s(200.33, s).haze_min_weight;
        assert!((min_weight(0.25) - 0.7875).abs() < 1e-4);
        assert!((min_weight(0.75) - 0.9).abs() < 1e-6);
        let lim1 = |s| regular_s(200.33, s).luminance_limit;
        assert!((lim1(0.25) - 0.4).abs() < 1e-3);
        assert!((lim1(0.75) - 0.35).abs() < 1e-6);
        let lim3 = |s| regular_s(80.0, s).luminance_limit;
        assert!((lim3(0.25) - 0.52).abs() < 1e-3);
        assert!((lim3(0.75) - 0.5).abs() < 1e-6);
        assert!((clear_s(80.0, 0.25).blur_radius - 0.375).abs() < 1e-6);
        assert!((clear_s(80.0, 0.75).blur_radius - 0.75).abs() < 1e-6);
        let wash_light = |s| {
            Recipe::for_size(
                200.33,
                Family::Regular,
                Appearance::Light,
                Translucency::new(s),
            )
            .wash_alpha
        };
        assert!((wash_light(0.25) - 0.1).abs() < 1e-6);
        assert!((wash_light(0.75) - 0.35).abs() < 1e-6);
    }

    #[test]
    fn m_clamps_to_the_measured_range() {
        // Laws evaluate at clamped m.
        assert_eq!(
            regular(10.0, Appearance::Dark),
            regular(34.33, Appearance::Dark)
        );
        assert_eq!(
            regular(500.0, Appearance::Dark),
            regular(200.33, Appearance::Dark)
        );
        assert_eq!(clear_s(10.0, 0.5), clear_s(40.0, 0.5));
        assert_eq!(clear_s(500.0, 0.5), clear_s(80.0, 0.5));
    }

    #[test]
    fn model_mirrors() {
        // §3 lobe: full shift at the silhouette, zero at depth h.
        assert_eq!(lobe(0.0, 20.0, 4.0), 4.0);
        assert_eq!(lobe(-20.0, 20.0, 4.0), 0.0);
        let mid_lobe = 4.0 * (1.0 - 0.75f32.sqrt());
        assert!((lobe(-10.0, 20.0, 4.0) - mid_lobe).abs() < 1e-6);
        // §3 level: 0 at b ≤ 0, log2-scaled above.
        assert_eq!(level(0.0), 0.0);
        assert_eq!(level(-3.0), 0.0);
        assert!((level(2.0) - 1.0).abs() < 1e-6);
        assert!((level(4.0) - 2.0).abs() < 1e-6);
        // Small radii take the 1 + b/2 branch.
        assert!((level(1.0) - 1.5f32.log2()).abs() < 1e-6);
        // §3 falloff: 1 at −2, 0 at 2, 0.5 at 0.
        assert!((falloff(-2.0) - 1.0).abs() < 2e-3);
        assert!(falloff(2.0).abs() < 2e-3);
        assert!((falloff(0.0) - 0.5).abs() < 1e-6);
        assert!(falloff(-1.0) > 0.5 && falloff(1.0) < 0.5);
        // §4.3 piecewise: holds at ends, steps on equal breakpoints.
        let xs = [-20.0, -1.0, 0.0, 0.0];
        let ys = [0.8, 0.4, 0.5, 1.0];
        assert_eq!(piecewise(&xs, &ys, -30.0), 0.8);
        assert_eq!(piecewise(&xs, &ys, 0.5), 1.0);
        assert!((piecewise(&xs, &ys, -10.0) - 0.5895).abs() < 1e-3);
        // §4.6 grade identity at (0, 1, 1).
        let id = Grade::new(0.0, 1.0, 1.0);
        let c = [0.3, 0.5, 0.7];
        let o = id.apply(c);
        for i in 0..3 {
            assert!((o[i] - c[i]).abs() < 1e-6, "{o:?}");
        }
        // A luma remap: (0.125, 1.125, 1) lifts dark, keeps white.
        let lift = Grade::new(0.125, 1.125, 1.0);
        assert!((lift.apply([0.0; 3])[0] - 0.125).abs() < 1e-6);
        let w = lift.apply([1.0; 3]);
        assert!((w[0] - 1.125).abs() < 1e-6);
        // §4.6 grade composite: at wash 0 and mix 1 it is the grade.
        let m = Grade::new(0.125, 1.125, 1.3);
        let comp = grade_composite(&m, [0.0; 3], 0.0, 1.0, [0.5; 3]);
        let direct = m.apply([0.5; 3]);
        for i in 0..3 {
            assert!((comp[i] - direct[i]).abs() < 1e-6);
        }
        // The wash replaces the graded colour by its alpha.
        let washed = grade_composite(&m, [1.0; 3], 0.2, 1.0, [0.5; 3]);
        assert!((washed[0] - 0.2f32.mul_add(1.0 - direct[0], direct[0])).abs() < 1e-6);
    }

    #[test]
    #[should_panic(expected = "outside [0, 1]")]
    fn translucency_rejects_out_of_range() {
        let _ = Translucency::new(1.5);
    }

    #[test]
    fn tinted_element_keeps_the_tint() {
        let r = regular(80.0, Appearance::Dark).with_tint(Tint {
            rgb: [1.0; 3],
            alpha: 0.3,
        });
        let t = r.tint.expect("tinted");
        assert_eq!(t.rgb, [1.0; 3]);
        assert_eq!(t.alpha, 0.3);
    }
}
