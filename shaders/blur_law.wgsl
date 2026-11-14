// §3: the blur a radius of `b` makes — the capture convolved with a
// Gaussian of standard deviation `BLUR_SIGMA_PT_PER_RADIUS · b` pt
// [fitted], whatever the capture scale; each shader carries it onto its
// capture's texels. Shared by the standalone and the engine material
// shaders, each of which realizes the Gaussian on its own capture.
const BLUR_SIGMA_PT_PER_RADIUS: f32 = 1.6666666;
// §4.4: the haze's Gaussian deviation, pt, per unit of haze radius
// [fitted].
const HAZE_SIGMA_PT_PER_RADIUS: f32 = 2.0;

// §3: the deviation, in capture texels, of the Gaussian the ambient
// sample (§4.7) reads for a radius of `b` capture texels: the one a
// trilinear read of the 2 × 2-mean chain at the radius's level makes. With
// `x = b < 2 ? 1 + b/2 : b` that read has variance (x² − 1)/12 from the
// reductions and x²/6 from the bilinear read, `x²/4 − 1/12` in all; at
// `x = 1` it is the plain bilinear read's 1/6.
fn ambient_sigma(b: f32) -> f32 {
    let x = max(select(b, 1.0 + b * 0.5, b < 2.0), 1.0);
    return sqrt(x * x * 0.25 - 1.0 / 12.0);
}
