# Glass material specification

This document specifies the glass material this crate implements: what a glass
element draws, the per-pixel model that produces it, the recipe each element
uses, and the scenes that verify an implementation.

## Conventions

- Lengths are points (pt). The size driver of an element is
  `m = min(width, height)` of its bounds.
- The blur and haze radii (§3, §4.4) set Gaussian deviations in points,
  whatever the element's capture scale. The ambient blur radius (§4.7) is
  in texels of the element's backdrop capture.
- Colours are 8-bit or unit-range encoded sRGB unless a section says linear.
- `sat(x)` clamps to [0, 1]. `mix(a, b, t) = a + (b − a)·t`.
- The field `d` is the signed distance to the element's silhouette, negative
  inside. `n` is its unit outward normal.

Every value and every form carries one of these tags:

| Tag | Meaning |
|---|---|
| **measured** | A value observed from the target appearance. |
| **fitted** | A law or form fitted to observations. A fitted size law reproduces every observed size exactly and is not claimed outside the observed range; a fitted form, with the rest of the model, reproduces the pixels of §6 within the 2-level difference between the two observed displays. |
| **chosen** | A starting value or form we picked. It is a property of this specification, not an observation, and is the first thing to revise when an observation contradicts it. |

Where the observations do not determine a value, the specification says so as
an open question. An implementation that needs a value there states its own
choice and tags it **chosen**.

## 1. Families and settings

A glass element belongs to one of two **families**:

- **Regular**: a frosted surface that hides detail behind it and changes with
  the appearance.
- **Clear**: a nearly transparent lens that keeps the content behind it
  legible and is identical in both appearances.

Two settings outside the element select the recipe:

- **Appearance**, dark or light. It changes the regular family only (§5.3).
- **Translucency**, a user setting from 0 to 1, default 0.5. It changes both
  families (§5.4). It is chosen by the user and does not follow the
  content behind the glass.

A recipe never depends on the backdrop's content [fitted: one recipe per
element reproduces the §6 interiors over black, grey and white]. The rendered
pixels follow the backdrop because the operations below read it.

## 2. Structure of an element

An element draws, bottom to top:

1. **The cast shadow**, outside its silhouette (§4.10).
2. **The glass body**: the backdrop behind the element, captured once,
   downsampled to the element's capture scale and resampled with refraction,
   depth-dependent blur, haze, grading, ambient pickup, contour shade and
   border correction (§4.1–§4.11). Only pixels inside the silhouette, with
   antialiased coverage, are drawn.
3. **The highlight layer**: a two-light rim highlight (§4.12).
4. **The tint layers**, on tinted elements only (§4.13).
5. **Content**, with the foreground dispersion of §4.14.

The material composites over the backdrop in encoded sRGB space [fitted: the
cast shadow's measured fringe matches an encoded-space blend and not a linear
one, §6].

### Shape field and groups

The field `d` of a standalone element is the signed distance of its shape: a
rounded rectangle with continuous or circular corners, a capsule or a circle.
The **ellipse bend**, which bends the normal in the corners toward the
inscribed ellipse's, is 0.5 on the large rounded card and on the capsule of element 3
(§7), and 0 on every other verification element [measured]; its dependence on
shape is open.

Elements in a **container** share one backdrop capture and one field: the
union of the members' fields, joined smoothly over a smoothing distance of
30 pt. A standalone element's smoothing is 8 pt. Both are measured. All
members render with one recipe, the container recipe of §5.6 [measured].
Inside the union, each pixel takes the tint and the ellipse bend of the member
whose own field is smallest there [chosen]. Gaps of 16 pt between container
members stay open at rest, and members brought together merge into one
continuous shape [measured].

## 3. Lobe profile and blur level

Four operations displace a backdrop sample along the normal by a
quarter-circle lobe [chosen]:

```
lobe(d, depth, shift) = shift · (1 − sqrt(t·(2 − t))),   t = sat(−d / depth)
```

The displacement is the full `shift` at the silhouette, where the profile has
a vertical tangent, and falls to 0 with zero slope at `depth` inside it. A
negative shift pulls the sample inward, which magnifies.

A blur radius `b` blurs the capture by a Gaussian of standard deviation
`σ = 5/3·b` pt [fitted: the clear card at `b` = 0, 0.375 and 0.75 over the
grid and the grid over the split, §6, where a capture texel is 2/3 pt and
`σ` is `2.5·b` texels]. The deviation is a length on the backdrop: the
capture scale sets only the resolution at which the capture realizes it
[fitted: the black | white edge through the large card, captured at 0.25,
§6]. Every observation was made at 3 px/pt, so whether the deviation
follows points or device pixels is open; this specification takes points
[chosen]. The law is fitted for `b ≤ 0.75`; the regular family's radii
reach 5, and this specification extends the law to them [chosen]. Reading
the capture's mip chain at the level of `b` capture texels instead keeps up
to 2.3 times the measured contrast at `b = 0.75` (§6).

The ambient sample (§4.7) reads the capture at the level of its radius on
its mip chain of 2 × 2 means [chosen]:

```
level(b) = max(0, log2(b < 2 ? 1 + b/2 : b))
```

Two shadow terms weight by a smooth step `falloff(x)` that runs from 1 at
`x = −2` to 0 at `x = 2` [fitted: the cast shadow fringe of §6]:

```
falloff(x) = 0.5 + x·(−0.5605 + x²·(0.16821 + x²·(−0.034454 + x²·0.0029545)))
```

## 4. The per-pixel model

The operations below run in this order for each pixel of an element. `p` is
the pixel, `C` the working colour (encoded, unpremultiplied).

### 4.1 Capture

The backdrop under the element's bounds plus the reach of its samples is
captured at the element's **capture scale** and mip-mapped. A capture texel
is the mean of the device pixels it covers [fitted: at blur radius 0 the
model reproduces the clear card's interior over the grid over the split
within 0.3 levels, §6]. The capture reads the backdrop through a chain of
2 × 2 means, so a texel weights the pixels under it near equally even where
a level does not halve the one below it evenly [chosen]. A capture whose
texels weight some pixels of a periodic backdrop more than others shifts
the backdrop's mean by an amount that drifts across the capture; over the
grid the large regular card showed that drift as a 2-level bowl (§6). All
samples below read this capture at a displaced
point, blurred by a radius or at a level (§3). A container is captured once,
under the union of its members' bounds, at the capture scale of its recipe
(§5.6).

### 4.2 Refraction

Inside the silhouette, the **primary sample** is taken at
`p + n·lobe(d, primary depth, primary shift)`. When the **rim mix** is
positive, a **rim sample** is taken at `p + n·lobe(d, rim depth, rim shift)`
and mixed in:

```
C = mix(primary, rim, rim mix · sat((d − r0) / (r1 − r0)))
```

with the **rim ramp** `(r0, r1)`. Refraction happens only inside the
silhouette; the rim sample is a second view from inside, not a bend of the
content around the element [chosen]. The ramp is (−1, 0) pt [measured], so
the rim sample's weight rises from 0 to the rim mix over the last point
before the edge.

The displacement profile through the clear card has not been observed: grid
lines through it are lost at its top edge, and a mid-card line is shifted by
about −0.17 pt [measured]. The lobe's shape against an observed displacement
is open.

### 4.3 Depth-dependent blur

The primary and rim samples are blurred (§3) by the radius
`blur radius · g(d')`, where `d' = d + lobe(…)` is the sample's refracted
depth and `g` interpolates the **blur gains** `g0…g3` linearly between the
**blur depths** `d0…d3`, keeping `g0` below `d0` and `g3` above `d3`; equal
consecutive depths make a step [chosen].

### 4.4 Haze

The primary sample is joined by the **haze** `F`, a blurred estimate of the
backdrop: the capture at the pixel, blurred by a Gaussian of standard
deviation `2·r` pt for the **haze radius** `r`, and unpremultiplied
[fitted: the black | white edge through the 60 pt circle and the
160 × 80 pt capsule, captured at 0.5, and through the large card and the
circle in its container, captured at 0.25, §6]. Like the §3 deviation it
does not follow the capture scale. A deviation of `r` capture texels is
5.3 pt on the circle and 10.7 pt on the card: the circle's dark side then
stays within 4 levels of black until 10 pt from the edge, where the
device's has risen from 38 at 28 pt to 64. The haze is not read at the
level of its radius either: the 2 × 2 means at level 3 alias the grid's
24 px period into a 32 pt ripple through the large card (§6). Then
[chosen]:

```
C1 = a · min(C, F) + b · max(C, F) + (1 − a − b) · C
C  = mix(C1, F, haze mix)
```

with the **min weight** `a`, the **max weight** `b` and the **haze mix**. This
runs before the rim-sample mix of §4.2.

### 4.5 Luminance limit

Before the grade, a bright sample is pulled down [fitted: with the ambient
gate of §4.7 the model reproduces every measured interior of the large cards,
§6]:

```
Y   = 0.2126·C.r + 0.7153·C.g + 0.0722·C.b
a   = sat(1 − Y·(1 − L))
C'  = mix(vec3(a·Y), a·C, 1 + (1 − a)·0.3)
```

where `L` is the **luminance limit**. At `L = 1` the step is the identity.
Whether the limit differs on SDR output is open: for the light appearance
0.94 is a candidate value there, and this specification uses 1.

### 4.6 Grade

The **grade** acts on the encoded, unpremultiplied colour [fitted: the
interiors of §6]. It maps luma onto [**floor**, **ceiling**] and scales
chroma by **chroma**:

```
Y' = (ceiling − floor)·Y + floor                // luma remap
c' = chroma · (c − 0.5) + 0.5                   // Cb, Cr centred at 0.5
graded = YCbCr⁻¹(Y', c')
C = mix(C, (1 − wash.a)·graded + wash.a·wash.rgb, grade mix)
```

with the BT.709 YCbCr transform (`Y = 0.2126 R + 0.7152 G + 0.0722 B`,
`Cb = (B − Y)/1.8556 + 0.5`, `Cr = (R − Y)/1.5748 + 0.5`). The **wash**, a
colour with an alpha, is composited over the graded colour by that alpha.

### 4.7 Ambient pickup

A third sample, the **ambient sample**, is pushed outward by
`lobe(d, ambient depth, ambient shift)`, blurred at the level of its own
**ambient blur** radius, graded by the **ambient grade** (same construction as
§4.6, no wash) and mixed in, so the surroundings' colour reaches the interior:

```
Yg = 0.2125·C.r + 0.7153·C.g + 0.0721·C.b
k  = favours light ? Yg : 1 − Yg
w  = ambient strength · (sat((d − e0)/(e1 − e0)) · k²)²
C  = mix(C, ambient, w)             // the ambient colour is not clamped
```

with the **ambient ramp** `(e0, e1) = (1, 0)` [measured], which is 1 through
the whole interior. The **ambient gate** has two polarities: `k = 1 − Yg`
weights the pickup toward dark pixels, `k = Yg` (favours light) toward bright
ones. The squared, linear-in-luma gate and its two polarities are fitted:
together with §4.5 they reproduce every measured interior of the large cards
(§6).

### 4.8 Contour shade

A soft band along the silhouette, moved down by the **contour shade shift**,
darkens the glass body [chosen]:

```
S(q) = falloff(4·sat(0.17676·q + 0.5) − 2)
αR   = density · wR · sat(S(dR / s) − S((dR + width) / s)) · mix(1, coverage, clip)
result = αR·black + (1 − αR)·result
```

`dR` is the field of the shape moved by the shift and `wR` the shape weight of
that evaluation (1 for a standalone element), `density` the **contour shade
density**, `s` its **softness** and `width` its band **width**. The band lies
just inside the moved contour. With **clip** 1 it is limited to the glass
coverage, so it darkens a band along the sides and a few points below the top
edge, and the part of the moved contour below the element is not drawn. The
shift's direction is open; this specification takes it downward, like the
cast shadow's [chosen]. At density 0.06 the band is at most 2 levels deep: on the
large card over black, the rim 8 pt inside the top edge reads 31 against 32
in dark appearance and 134 against 136 in light [measured], which cannot be
separated from the rim's own falloff.

### 4.9 Border correction

Two opposing directional lobes darken or lighten the glass body in a thin band
at the edge [chosen]. The correction has a **strength**, a **bearing**, a
band **width** and **offset**, an angular **aperture** and a **colour bias**.
Each lobe is a saturated directional ramp `q` shaped as
`q / max(1 + strength·(1 − q), ε)`; the two lobes are summed into a weight `h`
inside a narrow antialiased depth band, and the colour is biased:

```
C = C · (1 + colour bias · h · (3 − 2·C))
```

The band's exact depth window and ramp are open.

### 4.10 Cast shadow

Outside the silhouette the element casts a plain black shadow, independent of
the backdrop, whose weight falls off from the shape moved by the **cast
shift** [fitted: `α` reproduces every measured fringe sample above and below
the large card, §6]:

```
d2 = field of the shape translated by the cast shift
α  = w2 · cast density · cast gain · falloff(4·sat(0.25·d2 / cast falloff + 0.5) − 2)
```

`w2` is the shape weight of the shifted evaluation (1 for a standalone
element), the **cast density** a size-dependent factor, the **cast gain** a
per-family factor and the **cast falloff** the length over which the weight
falls off. Without the gain the model
gives 154 at 1 pt below the large card on white in dark appearance, against
the measured 225.

The clear family casts no shadow [measured].

### 4.11 Output

Inside the silhouette the glass body has alpha 1 and is composited by the
antialiased coverage. Each colour channel is limited to the **channel
ceiling**, then to the **display peak**, the largest encoded value the target
holds: 1.0 on an SDR target, 1.2 on the observed HDR display
[measured]. On an SDR target both limits sit above the 1.0 the target holds.

Each element has an **exterior span** (§5): the distance outside the
silhouette over which the element's field is evaluated.

### 4.12 Highlight layer

Above the glass body, a separate layer adds a white rim highlight from two
lights, a **first** and a **second**:

- **Bearings.** The bearing θ points a light along `(sin θ, −cos θ)` in a
  y-down space: 0 lights the top edge, π the bottom edge. The first light is
  at 0 and the second at π [fitted: the rim on the large card is equally strong on its
  top and bottom edges, §7 Rim].
- **Gains** 0.5 and 0.5, **inner fade** 0.75 (the share of a band's weight
  lost by its inner edge), **colours** white at alpha 1 [measured].
- **Aperture** π/2 on the large card (m = 200.33) and 4π/9 on every other
  verification element [measured]. The rule between them is open.
- **Glow.** A second, broader band whose gain, width and aperture
  multipliers are 0.15, 8 and 0.65 [measured]. Its form is open.
- The layer acts on the colour already accumulated under it, within the
  highlight's coverage [chosen]. The rim it produces is about 1 pt thick
  [measured].
- Whether the highlights respond to device motion is open: no observation was
  made with the device tilted.

### 4.13 Tint layers

A tinted element adds two layers over the highlight: a fill of the shape in the
tint colour, and an edge gradient of the tint colour at full alpha from −1 to
0 pt, falling to alpha 0 at 10 pt [measured stops]. The strength with which
the two layers composite, relative to the tint's declared opacity, is open.

### 4.14 Foreground dispersion

Content inside the glass is refracted by its own lobe and split into a
spectral fringe of seven taps along a dispersion axis, with an edge opacity
fade [chosen]. Its parameters are open. Whether the glass body's primary
sample takes a similar spectral path is open.

## 5. Recipes

Values are for translucency 0.5, the default. §5.4 gives the dependence on the
setting.

### 5.1 Regular family

| Parameter | Law or value | Tag |
|---|---|---|
| capture scale | 0.5 for 34.33 ≤ m ≤ 80; 0.25 at m = 200.33. Between 80 and 200.33 open. | measured |
| blur radius | 5 | measured |
| blur depths | −0.5·m, −1, 0, 0 | fitted (0) / measured |
| blur gain 0 | 0 at m ≤ 60; 0.133 at m = 80; 0.8 at m = 200.33. `0.8·sat((m − 56)/144)` passes through the last two points; it is one of many such curves, and every value between the measured sizes is open. | measured |
| blur gain 1 | half of blur gain 0 | fitted |
| blur gains 2, 3 | 0.5, 1 | measured |
| primary shift | −min(0.5·m, 60) | fitted |
| primary depth | min(0.25·m, 20) | fitted |
| rim shift | max(16, 0.25·m) | fitted |
| rim depth | max(16, 0.2·m) | fitted |
| rim ramp, rim mix | (−1, 0), 0.6 | measured |
| grade and wash | §5.3 | measured |
| grade mix | 1 | measured |
| luminance limit, dark | 0.6 at m ≤ 60, 0.5 at m = 80, 0.35 at m = 200.33. Between them open. | measured |
| luminance limit, light | 1 | measured |
| ambient shift, ambient depth | 0.35·m | fitted |
| ambient blur | 0 at m ≤ 60; 0.35·m at m = 80 and m = 200.33. Between 60 and 80 open. | measured / fitted |
| ambient strength | 0 at m ≤ 60; dark 0.133 at m = 80 and 0.8 at m = 200.33; light 0.083 and 0.5. These are `0.8·sat((m − 56)/144)` and `0.5·sat((m − 56)/144)` at those points, with the same caveat as blur gain 0. | measured |
| ambient ramp | (1, 0) | measured |
| ambient grade | §5.3 | measured |
| haze radius | 8 | measured |
| haze min weight, max weight | §5.3 | measured |
| haze mix | 0.5 | measured |
| contour shade density, shift, softness, width, clip | 0.06, 8, 5, 4, 1 | measured |
| border strength, bearing, band width, band offset, colour bias | 0.4, π/2, 0.533, −0.533, −0.3 | measured |
| border aperture | §5.3 | measured |
| cast shift | (0, 8) | measured |
| cast falloff | 4 at m ≤ 60, 7.33 at m = 80, 24 at m = 200.33. Between them open. | measured |
| cast density, dark | 0.04 at m ≤ 60, 0.133 at m = 80, 0.6 at m = 200.33. Between them open. | measured |
| cast density, light | 0.04 at m ≤ 60, 0.1 at m = 80, 0.4 at m = 200.33. Between them open. | measured |
| cast gain | 0.3 | fitted (§4.10) |
| channel ceiling | §5.3 | measured |
| exterior span | 8.89 at m ≤ 60; 9.11 dark, 8.89 light at m = 80; 39.83 dark, 37.90 light at m = 200.33. Between them open. | measured |
| highlight aperture | 4π/9 at m ≤ 80, π/2 at m = 200.33. Between them open. | measured |

The fitted laws hold over the measured range 34.33 ≤ m ≤ 200.33. Behaviour
above m = 200.33 is open; in particular no cap of the rim lobe has been
observed.

### 5.2 Clear family

| Parameter | Law or value | Tag |
|---|---|---|
| capture scale | 0.5 | measured |
| blur radius | 0.75 | measured |
| blur depths | −0.5·m, −1, 0, 0 | fitted / measured |
| blur gains | 1, 0.5, 0.5, 1 | measured |
| primary shift | −0.65·m | fitted |
| primary depth | min(0.36·m, 20) | fitted |
| rim shift, depth | 0.2·m, 0.125·m | fitted |
| rim mix | 0 | measured |
| grade floor, ceiling, chroma | 0.125, 1.08, 1.06 | measured |
| wash | white at alpha 0 | measured |
| luminance limit | 1 | measured |
| ambient pickup | off: shift, depth, blur and strength 0 | measured |
| ambient grade floor, ceiling, chroma; gate | 0.75, 1, 1.2; favours light (inert while the pickup is off) | measured |
| haze | off: radius and weights 0 | measured |
| contour shade | off | measured |
| cast shadow | off: density 0 | measured |
| cast gain | 0.1 (inert while the shadow is off) | measured |
| border | as the regular family, aperture 75° | measured |
| channel ceiling | 1.192 | measured |
| exterior span | 1.533 | measured |
| highlight aperture | 4π/9 | measured |

The fitted laws hold over 40 ≤ m ≤ 80. Behaviour above m = 80 is open.

### 5.3 Appearance

The regular family switches these values with the appearance; the clear family
switches none [measured]:

| Parameter | Dark | Light |
|---|---|---|
| grade floor, ceiling, chroma | 0.125, 1.125, 1.3 | 0.4, 1.03, 1.2 |
| wash | black at alpha 0 | white at alpha 0.2 |
| luminance limit | by size (§5.1) | 1 |
| ambient grade floor, ceiling, chroma | 0.125, 0.5, 1 | 0.9, 1, 1.2 |
| ambient gate | `1 − Yg` | `Yg` (favours light) |
| haze min weight, max weight | 0.9, 0 | 0, 0.9 |
| channel ceiling | 1.308 | 1.070 |
| border aperture | 75° | 96° (106° on SDR output) |
| ambient strength, cast density, exterior span | by size (§5.1) | by size (§5.1) |

Highlight bearings and colours do not switch.

### 5.4 Translucency setting

Values at the setting `s ∈ [0, 1]`. Rows marked *linear* interpolate linearly
between their values at 0, 0.5 and 1; observations at 0.25 and 0.75 lie on
those lines [measured]. Rows not marked *linear* were observed at 0, 0.5 and
1 only, and their values between those points are open.

| Parameter | s = 0 | s = 0.5 | s = 1 | |
|---|---|---|---|---|
| regular wash, dark | grey 0.125 at alpha 0 | black at alpha 0 | grey 0.125 at alpha 0.5 | linear (alpha) |
| regular wash, light | white at alpha 0 | white at alpha 0.2 | white at alpha 0.5 | linear |
| regular capture scale, m = 200.33 | 0.333 | 0.25 | 0.125 | |
| regular capture scale, m ≤ 80 | 0.5 | 0.5 | 0.125 | |
| regular haze min weight (dark), max weight (light) | 0.675 | 0.9 | 0.9 | linear |
| regular haze mix | 0 | 0.5 | 1 | linear |
| regular luminance limit, dark, m = 200.33 | 0.45 | 0.35 | 0.35 | linear |
| regular luminance limit, dark, m = 80 | 0.54 | 0.5 | 0.5 | linear |
| regular luminance limit, dark, m ≤ 60 | 0.6 | 0.6 | 0.6 | |
| clear blur radius | 0 | 0.75 | 0.75 | linear |
| clear blur gains | 0, 0, 0, 0 | 1, 0.5, 0.5, 1 | 1, 0.5, 0.5, 1 | |
| clear haze radius, mix | 0, 0 | 0, 0 | 8, 1 | linear (mix) |

Grade floor, ceiling and chroma, and the clear family's wash, do not move with
the setting.

### 5.5 Tinted and interactive elements

A tint adds the layers of §4.13. Element 3, the only tinted and the only
m = 80 regular element, matches every fitted law at its size; whether its tint
or its interactivity contributes to its size-specific values (cast falloff
and density, luminance limit, exterior span) is open.
Interaction does not change the recipe [measured].

The interactive material (the `.interactive()` behaviour) never changes its
geometry on press or drag: a 200 × 80 pt tinted element, a
200 × 80 pt untinted element, a 66 × 34 pt element and a plain-style button all
kept a growth of 1.000 through a press and through a drag [measured]. Any press
feedback of the material that is not geometry is unchanged.

Growth belongs to the button control, not to the material: §8.1 specifies it.
The grown button keeps its idle recipe [measured].

### 5.6 Containers

The members of a container render with one recipe, the **container recipe**,
built from the recipes they would have standing alone. *Largest* and
*smallest* are the members with the largest and the smallest `m`:

| Parameter | Value | Tag |
|---|---|---|
| capture scale | the largest member's | measured |
| primary shift, primary depth, blur depth 0 | the smallest member's | measured |
| every other value of §5.1 | the largest member's | measured |

Because the capture scale never grows with `m`, the container's capture scale
is the smallest among its members' recipes. The members' order does not
matter, and the container's own bounds are not a size driver [measured].

Observed on device (dark appearance, translucency 0.5), as interior means of
the centre half of each member's frame. Each 60 pt circle is measured
standing alone and as a member of the container named in its row:

| Circle's container | Backdrop | Circle alone | Circle in container | Largest member, alone / in container |
|---|---|---|---|---|
| with a 300 × 200.33 pt card (radius 40), 40 pt below it | white | 186.3 | 122.5 | 122.7 / 122.7 |
| | grey 0.5 | 134.7 | 116.6 | 117.1 / 117.1 |
| | black | 31.5 | 31.5 | 31.5 / 31.5 |
| with a 160 × 80 pt capsule, 50 pt beside it | white | 186.4 | 160.4 | 160.5 / 160.5 |
| | grey 0.5 | 134.8 | 127.5 | 127.7 / 127.7 |
| with a second 60 pt circle, the two spanning 200 × 200 pt | white | 186.4 | 186.4 | — |

Over a vertical black | white edge through the circle's centre, the circle
beside the card steps from 84 to 136 within 2 pt when it stands alone; in the
container its interior ramps over about 13 pt, as the card's does, and it
casts the card's shadow. The card and the capsule render the same in their
containers as alone, to within 1 level. The container with the card captures
at 0.25 and the circle alone at 0.5; the containers with the capsule or a
second circle capture at 0.5. In both mixed containers the primary shift and
depth are −30 and 15, the circle's, where the card alone has −60 and 20 and
the capsule −40 and 20; listing the circle first or last changes nothing.

In both mixed containers the member with the largest `m` is also the largest
by area, so whether the larger member is chosen by `m` or by area is open.
Only regular containers were observed: a clear container, one mixing the two
families, or one with tinted and untinted members has not been, and which tint
each member draws is chosen (§2).

## 6. Model checks

The model above, with the §5 recipes, against measured pixels. Shadow
samples lie on the vertical centre line of the large card; interiors are the
mean of the centre half of an element's frame (§7 layout).

**Cast shadow around the large card** (§4.10; encoded-space blend over the
backdrop, cast gain 0.3, cast falloff 24):

| Sample | Model, dark / light, white | Measured | Model, dark / light, grey | Measured |
|---|---|---|---|---|
| 1 pt below | 225 / 235 | 225 / 235 | 113 / 118 | 113 / 118 |
| 8 pt below | 232 / 240 | 232 / 240 | | |
| 20 pt below | 244 / 248 | 244 / 248 | 122 / 124 | 122 / 124 |
| 1 pt above the top | 241 / 246 | 241 / 246 | | |

A linear-space blend of the same weight would give 241 at 1 pt below on dark
white, against the measured 225.

**Interior** (§4.5–§4.7 on a uniform backdrop, where every sample equals the
backdrop):

| Element, backdrop | Model | Measured |
|---|---|---|
| regular #1 dark, black | 32 | 32 |
| regular #1 dark, grey | 116 | 116 / 115 |
| regular #1 dark, white | 122 | 122 |
| regular #1 dark, white, translucency 0 | 146 | 146 |
| regular #1 dark, white, translucency 1 | 86 | 86 |
| regular #1 light, black | 136 | 136 |
| regular #1 light, grey | 205 | 203 / 205 |
| regular #1 light, white | 255 (clamped) | 254 / 255 |
| clear #2, black | 32 | 32 |
| clear #2, grey | 154 | 154 |
| clear #2, white | 255 (clamped) | 255 |

Values `a / b` are the two observed displays.

**Blur through the clear card** (§3, §4.1, §4.3; the model applied to the
captured bare backdrop, rows of the card's interior where refraction does
not reach, `x` from 20 pt inside either end): the luma standard deviation
along a row, at translucency 0, 0.25 and 0.5 (blur radius 0, 0.375, 0.75).

| Backdrop, row | Radius | Model | Measured | Level read (§3) |
|---|---|---|---|---|
| grid over the split, centre row | 0 | 18.69 | 18.72 | 18.69 |
| grid over the split, centre row | 0.375 | 11.80 | 12.28 | 16.52 |
| grid over the split, 2.7 pt below centre | 0.375 | 13.14 | 12.87 | 16.93 |
| grid over the split, centre row | 0.75 | 8.49 | 8.65 | 14.82 |
| grid over the split, 2.7 pt below centre | 0.75 | 9.84 | 9.77 | 15.59 |
| grid, 1 pt below a line (the centre row) | 0.75 | 12.42 | 12.06 | 23.51 |
| grid, midway between lines | 0.75 | 8.19 | 8.54 | 19.68 |

The last column reads the capture's mip chain at `level(b)` instead; at
radius 0.75 it keeps 1.7 to 2.3 times the measured contrast.

**Large regular card over the grid** (§4.1, §4.4; dark and light appearance,
translucency 0.5): the luma standard deviation along the card's centre row,
`x` from 20 pt inside either end. The grid is not visible through the card,
and the observed card is flat.

| Appearance | Model | Measured |
|---|---|---|
| dark | 0.00 | 0.00 |
| light | 0.00 | 0.00 |

A capture texel that weights the device pixels under it unequally (§4.1)
gives 0.63 dark and 0.90 light, a bowl across the whole card; a haze read
at the level of its radius (§4.4) leaves a 32 pt ripple of 0.26 dark and
0.48 light.

**Black | white edge through the elements** (§3, §4.4; §7 scenes 4 and 5
over the edge, dark appearance, translucency 0.5): the luma of the 4 pt
band around each element's centre row, across 28 pt either side of the edge
(90 pt for the card), against the device capture of the same elements.
The 60 pt circle standing alone, every 2 pt from 28 pt left of the edge:

| | Profile |
|---|---|
| Model | 38 39 41 43 47 51 54 58 61 66 71 76 80 85 · 135 144 150 155 160 165 168 170 173 175 176 177 178 178 |
| Measured | 38 39 40 43 46 48 52 54 59 64 69 75 80 84 · 136 146 154 160 164 168 171 174 176 178 179 180 180 180 |
| Deviations in capture texels | 31 31 31 31 32 32 32 32 33 35 41 49 61 76 · 136 157 170 177 181 184 184 184 184 184 183 182 182 181 |

The mean absolute difference from the measured profile, in levels:

| Element | Capture scale | Model | Deviations in capture texels |
|---|---|---|---|
| 60 pt circle, alone | 0.5 | 2.3 | 13.1 |
| 160 × 80 pt capsule, alone | 0.5 | 1.7 | 10.6 |
| 60 pt circle, in the card's container | 0.25 | 2.1 | 3.6 |
| large card, alone | 0.25 | 2.2 | 2.7 |
| large card, in the container | 0.25 | 2.2 | 2.7 |

The last column counts both the §3 and the haze deviations in capture
texels, `2.5·b` and `r`. No single deviation in capture texels fits both
scales: the circle alone is closest at 3 texels per unit of haze radius
(2.3), where the card's difference grows to 4.3 and the container circle's
to 8.0. Of 1.67, 1.83, 2 and 2.17 pt per unit of haze radius, 2 is the one
that keeps every element within 2.3 levels. With the haze at 2 pt per unit
and the §3 deviation in capture texels, the card differs by 2.6 and the
container circle by 2.5.

## 7. Verification scenes

An implementation renders these scenes offscreen and the results are reviewed
by looking at the images. GPU output is not pixel-stable across adapters, so
scenes are not compared pixel by pixel; the observations below say what the
images must show, with the measured values alongside.

### Layout

A 402 × 874 pt scene at 3 px/pt with twelve elements at these frames
(x, y, width, height). Rounded rectangles and capsules have continuous
corners, a capsule's radius being half its short side; the circles are
circular.

| # | Family | Shape | Frame |
|---|---|---|---|
| 1 | regular | rounded rectangle, radius 40 | 51, 51.67, 300, 200.33 |
| 2 | clear | rounded rectangle, radius 30 | 51, 276, 300, 80 |
| 3 | regular | capsule, white tint at opacity 0.3, interactive | 101, 380, 200, 80 |
| 4 | regular | capsule, interactive | 121, 484, 160, 55.67 |
| 5 | regular | capsule, interactive | 168, 563.67, 66, 34.33 |
| 6 | regular | container of three 60 pt circles at x = 0, 76, 152 (16 pt apart) | 95, 622, 212, 60 |
| 7 | regular | capsule | 45, 716.33, 40, 40 |
| 8 | regular | capsule | 101, 716.33, 80, 40 |
| 9 | regular | capsule | 197, 706.33, 160, 60 |
| 10 | clear | capsule | 45, 800.33, 40, 40 |
| 11 | clear | capsule | 101, 800.33, 80, 40 |
| 12 | clear | capsule | 197, 790.33, 160, 60 |

### Backdrops

Black; grey 0.5; white; a vertical red | blue split at the scene's centre; an
8 pt black grid on white with 0.5 pt lines; the grid over the split; a
vertical black | white edge at the scene's centre. The split uses red
(230, 38, 38) and blue (26, 90, 230), encoded [chosen].

### Scenes

1. Every backdrop in dark and in light appearance at translucency 0.5.
2. White in dark and light at translucency 0 and 1.
3. The container with its circles moved together until they touch, at rest.
4. The **mixed container**: a regular 300 × 200.33 pt rounded rectangle
   (radius 40) at (51, 120) and a regular 60 pt circle at (171, 360.33) in
   one container, with the same two shapes standing alone at (51, 480) and
   (171, 720.33), on its own 402 × 874 pt scene at 3 px/pt. Black, grey 0.5,
   white, the split and the edge, in dark appearance at translucency 0.5.
5. The **button scene** (§8.1), on its own 402 × 874 pt scene at 3 px/pt,
   over black and over white, in dark and in light appearance at
   translucency 0.5. One row per size group (mini, regular, large) holding a
   "Glass" text button, an icon-only button and an icon-and-title button;
   then, at the regular size, a prominent button, a disabled glass button, a
   disabled prominent button, a red-tinted glass and a red-tinted prominent
   button, a rounded-rectangle and a circle button, a two-line "Two line /
   label" button and a "Glass" button 322 pt wide.
6. The **press sequence**: the regular "Glass" glass button alone over black
   in dark appearance, touched at its centre at t = 0 and released at
   t = 1 s, rendered at t = 0, 0.147, 0.5, 1.215, 1.43 and 2 s.
7. A regular 160 × 80 pt capsule standing alone at (121, 397), its centre on
   the edge, on its own 402 × 874 pt scene at 3 px/pt, in dark appearance at
   translucency 0.5.
12. The **segmented scene** (§8.4), on its own 402 × 874 pt scene at
   3 px/pt, over black, grey 0.5 and white, in dark and in light
   appearance, each control 300 pt wide: "One | Two | Three" with the first
   segment selected, at the regular and at the large size; "One | Two" with
   the second selected; "A | Bb | Ccc | Dddd" with the third selected;
   "Short | A much longer label" with the first selected; three symbol
   segments; a red-tinted and a disabled "One | Two | Three".
13. The **selection sequence**: a 300 pt "One | Two | Three" control alone
   over black in dark appearance, its selection moved from the first to
   the third segment at t = 0, rendered at t = 0, 0.05, 0.1, 0.2, 0.38 and
   0.6 s.

### Observations

- **Frost.** Through the large regular card the grid is not visible; through
  the clear card it is, at the same pitch as outside, with the lines bent in
  the last few points before the edge, and softened: each line spreads over
  about 4 pt, and a row midway between two lines varies with a luma standard
  deviation of 8.5, against about 25 at translucency 0 [measured, over the
  observed grid, whose lines are 2 px at luma 40].
- **Appearance.** Over black, the regular card's interior is dark grey in
  dark appearance (32) and light grey in light (136); the clear card is dark
  grey (32) in both. Over white, dark regular is mid grey (122) and light
  regular is white.
- **Colour through the glass.** Over the split, each card shows red on its
  left and blue on its right, in both families and appearances.
- **Rim.** A thin bright line, about 1 pt, on the top and the bottom edge of
  each element, equally strong (measured on the large card). The model
  expects it to fade toward the left and right sides.
- **Shadow.** Over white and grey, a soft dark fringe below the large regular
  card, darkest against the edge, still visible 20 pt below (244 on white in
  dark appearance) and fading out by about 56 pt; a fainter one above the top
  edge. The model expects the small regular elements' fringe (density 0.04,
  falloff 4) to be at most about 3 levels deep on white and to end about 16 pt
  below the edge. None around the clear elements. Over black, nothing outside
  any element.
- **Translucency.** Over white in dark appearance, the large regular card is
  lighter at 0 (146) and darker at 1 (86) than at 0.5 (122); in light
  appearance it stays white.
- **Sizes.** Elements 7 and 8 render the same recipe. Over white in dark
  appearance the model expects the small regular elements to be lighter than
  the large card (about 185 against 122), since their luminance limit is
  higher and they have no ambient pickup.
- **Union.** The container's circles stay separate at 16 pt; brought
  together they merge into one shape with no seam or hole.
- **Mixed container.** The circle in the container renders the card's
  recipe: over white its interior is mid grey like the card's (122), where the
  standalone circle is lighter (186), and over grey 117 against 135. Over the
  split it shows the card's soft transition from red to blue, where the
  standalone circle's is sharp. Over white and grey it casts the card's soft
  shadow, where the standalone circle's fringe is a few levels deep at most.
  The card renders the same in the container and standing alone.
- **Button.** Every button of the button scene has the frame of the §8.1
  tables, and its glass fills that frame exactly. Text buttons are capsules,
  the two-line one included; the rounded-rectangle button has small corners
  (6.9 pt) and the circle button is round. Over black, glass buttons are dark
  grey with a white label in dark appearance (32) and mid grey with a black
  label in light (132); the prominent button is solid accent blue with a
  white label, the red-tinted glass button has a red label and the
  red-tinted prominent button a red fill. Disabled labels are faint: over
  black, 82 on 32 in dark and 57 on 132 in light. Over white in dark, the
  prominent blue is visibly lighter than over black (32, 156, 248 against
  3, 145, 252).
- **Press.** In the press sequence the button is 1.263 times its idle size
  at 0.147 s, 1.245 times at 0.5 s, 0.932 times at 1.215 s and back to its
  idle size at 2 s. While pressed its interior is lighter at the centre than
  at the ends (about 98 against 73) and uniform from top to bottom; at
  1.215 s it carries no wash. The label scales with the button and stays
  white.
- **Edge.** Over the black | white edge, the standalone circle's dark side
  rises steadily from about 38 at 28 pt from the edge to 84 beside it, then
  steps to 136 and keeps rising to about 180: light from the white side
  reaches across the whole circle, and the capsule of scene 7 shows the same
  spread. In the container the circle ramps smoothly through the edge, as
  the card does.
- **Segmented control.** Each control of the segmented scene draws a
  capsule container 32 pt tall (49 pt at the large size), 1 pt taller than
  its frame, divided into equal segments whatever their labels, with no
  separators. The selected segment has a capsule thumb 2 pt inside its
  segment on every side, so the first of three is 96 × 28 pt at x = 2. The
  container is 28, 28, 31 over black and 222, 222, 225 over white in dark
  appearance, 14, 14, 15 and 238, 238, 239 in light; the thumb is 90, 90,
  95 and 226, 226, 231 in dark and white in light. Labels are white in dark
  and black in light, the selected one visibly heavier. The red tint
  changes nothing, and the disabled control is the plain one at half
  opacity, with labels of 128 over black in dark appearance.
- **Selection.** In the selection sequence the thumb's centre is at 50 pt
  from the container's leading edge at t = 0, then 89.5, 152, 229 and
  251.3 pt at 0.05, 0.1, 0.2 and 0.38 s, just past its end, and 250 pt at
  0.6 s; the thumb keeps its size throughout. "Three" is already in the
  heavier weight, and "One" in the lighter, from the first frame after the
  change.

## 8. Controls

A control is drawn from the glass elements of §2 to §5 and the chrome its
section specifies: geometry, states and motion. Lengths are layout points at
the default text size. Colours are encoded 8-bit sRGB as rendered on the
observed display, and "over black" or "over white" names the backdrop behind
the control. Unless a row says otherwise, observations are at translucency
0.5.

### 8.1 Button

A button is one standalone, interactive glass element of the regular family
(§5.1): its shape is the button's border shape, and its label is the
element's content (§2, item 5). A button is of one of two **kinds**:

- **Glass**: the glass body with the label drawn on it.
- **Prominent**: the glass body filled with the button's tint, with a white
  label.

The tint defaults to the accent colour, (0, 145, 255) in dark appearance and
(0, 136, 255) in light [measured].

#### Geometry

The button's frame is its label's layout size plus the insets of its size
group, and the glass fills that frame exactly: no part of the chrome reaches
outside it apart from the material's own cast shadow [measured: the drawn
silhouette matches the frame within 0.17 pt for every observed button].

| Size group | Label text | Line height | Horizontal inset | Vertical inset | Height, one text line |
|---|---|---|---|---|---|
| mini, small | 15 pt, regular weight | 18 | 10 | 5 | 28 |
| regular | 17 pt, regular weight | 20.33 | 12 | 7 | 34.33 |
| large, extra large | 17 pt, regular weight | 20.33 | 20 | 15 | 50.33 |

All values are measured. Mini and small are identical, and so are large and
extra large. The label text is identified by its ink: "Glass" inks
36 × 11.33 pt in the 28 pt button and 40.33 × 13 pt in the others, as plain
15 pt and 17 pt text do.

Observed frames (width × height) [measured]:

| Content | mini, small | regular | large, extra large |
|---|---|---|---|
| "Glass" | 57.67 × 28 | 66 × 34.33 | 82 × 50.33 |
| "Continue" | 82 × 28 | 92.67 × 34.33 | 108.67 × 50.33 |
| a filled star symbol | 39.33 × 28.33 | 46 × 34.67 | 62 × 50.67 |
| the symbol, then "Glass" | 85 × 28.62 | 96 × 35 | 112 × 51 |

- **Label placement.** The content is centred in the frame on both axes. A
  multi-line label is centre-aligned and the frame grows by its lines: the
  two-line regular "Two line / label" button is 86.33 × 56.33 pt. A button
  given a larger width keeps its label centred [measured].
- **Symbols.** A symbol is drawn at the label's text size, in the label's
  colour, with the same insets [measured]. An icon-and-title label places
  the symbol first, then 8 pt, then the title, centred vertically on each
  other [fitted: at both text sizes the content width is the symbol's layout
  width, 22 pt at 17 pt and 19.33 pt at 15 pt, plus 8 plus the title's].
- **Shape.** The default shape is a capsule whose radius is half the frame's
  height, for every size and for multi-line labels [measured: fitted corner
  radius equal to half the height within 0.17 pt, 28.17 for the two-line
  button]. Its corners are continuous, like the material's capsules [chosen:
  a circular fit leaves a residual of 0.2 to 0.6 pt, which does not
  separate circular from continuous corners]. A rounded-rectangle shape
  without a radius has radius 4.8 (mini, small), 6.9 (regular) and 8.9
  (large, extra large); with a radius it uses that radius [measured,
  circular fit: a requested 4 drew 3.8 to 4.2]. A circle shape replaces the horizontal inset by the vertical
  one and draws a circle of diameter `min(width, height)` centred in the
  frame: the regular star button is 36 × 34.67 pt with a 34.67 pt circle
  [measured].
- **Hit region.** The frame, extended to at least 44 × 44 pt about its centre
  [chosen: touch hit-testing was not observed; 44 pt is a comfortable
  minimum touch target and keeps the 28 pt buttons reachable].

#### Glass body

The body is an ordinary element of the regular family: it uses the recipe
of §5.1 at its own `m`, the frame's height for every button above [chosen:
the measured interiors below are those §5.1 gives at `m ≤ 60`, and §7
expects about 185 over white in dark appearance]. At `m` = 28, below §5.1's
measured range, the recipe of `m ≤ 60` applies [measured: the mini button's
interior over black equals the regular one's, 31 in dark appearance].
Measured interiors of the regular glass button:

| Appearance | Over black | Over white |
|---|---|---|
| dark | 32 | 184 |
| light | 132 | 252 |

#### States

| State | Glass kind | Prominent kind |
|---|---|---|
| enabled | label white in dark appearance, black in light | fill: the tint; label white |
| tinted | label in the tint colour | fill: the tint; label white |
| disabled | body unchanged; label dimmed | fill: grey; label dimmed |
| pressed | growth and wash (below) | growth and wash (below) [chosen] |

All entries are measured except the pressed prominent button, which was not
observed and is chosen to move as the glass kind does.

- **Dimmed label.** A disabled label is the colour beneath it plus 50 levels
  per channel in dark appearance and minus 75 levels in light, clamped
  [measured: the same offsets over black and white, on the glass body and
  on the disabled prominent fill: 32 → 82 and 184 → 234 in dark,
  132 → 57 and 252 → 177 in light].
- **Disabled prominent fill.** The tint is dropped: the fill is
  `mix(body, g, k)` with `g = 0.60, k = 0.20` in dark appearance and
  `g = 0.55, k = 0.25` in light [fitted to the fills 56 and 178 over black
  and white in dark, 134 and 224 in light].
- **Disabled presses.** A disabled button does not respond to touches
  [chosen: not observed].
- **Prominent fill.** Rendered fills over the four combinations [measured]:

  | Tint (its colour, dark / light) | Dark, over black | Dark, over white | Light, over black | Light, over white |
  |---|---|---|---|---|
  | accent (0, 145, 255) / (0, 136, 255) | 3, 145, 252 | 32, 156, 248 | 3, 106, 205 | 4, 136, 252 |
  | red (255, 67, 69) / (255, 56, 61) | 252, 66, 69 | 251, 101, 104 | 226, 25, 29 | 252, 56, 61 |
  | green (48, 210, 88) / (52, 199, 90) | 50, 209, 89 | 74, 210, 108 | 38, 159, 69 | 54, 198, 91 |
  | grey (143, 143, 149) | 143, 142, 149 | 158, 157, 162 | 111, 110, 117 | 142, 141, 148 |
  | encoded grey 0.5 (128) | 97 | 144 | 128 | 165 |
  | black | 0 | 0 | 0 | 15 |

  The fill follows the backdrop: the accent, red, green and grey tints
  render their own colour over black in dark appearance and over white in
  light, and shift toward the backdrop otherwise; the plain grey 0.5 shifts
  by about 0.3 of the distance to the glass body in every case. This
  specification draws the fill as the tint layer of §4.13 over the glass
  body at full strength [chosen: it reproduces the two matching cases and
  is within 50 levels of the others; the law of the shift is the open
  strength of §4.13], without the §4.13 edge gradient: no darkening toward
  the fill's edge was observed [measured], so the fill is flat out to the
  silhouette. The §4.12 rim highlight remains faintly visible at the top
  and bottom edges of the fill, about 8 levels above it [measured].
- **Tinted glass label.** The label takes the tint colour [measured:
  rendered red labels 238, 94, 99 and 252, 195, 197 over black and white in
  dark appearance, 174, 30, 37 and 222, 69, 76 in light]. Like the fill it
  shifts with the backdrop; this specification draws it in the tint colour
  [chosen: same open law as the prominent fill].
- **Focus.** Not observed: the reference device has no keyboard focus. An
  implementation that shows a focused state states its own choice.

#### Press motion

A **press progress** `p` drives the pressed state: 0 at rest, 1 pressed. A
touch that begins in the hit region drives `p` toward 1 by the **press
spring**. Release or cancellation drives it toward 0 by the **release
spring**. Each spring starts from the current `p` and its velocity [chosen:
only a full press from rest and a release from a settled press were
observed]. `p` starts at the touch event [chosen: the observed motion began
0.05 s after touch-down and 0.02 to 0.03 s after touch-up, which is input and
display latency, not part of the control].

Both springs act on a unit mass [fitted: three captures of the regular
button at 48 frames per second, width and height fitted separately; every fit
lies within 1.5 rad/s and 0.03 of these values, with a residual below 1.1 %
of the motion's amplitude]:

| Spring | Natural frequency ω0 | Damping ratio ζ | Stiffness ω0² | Damping 2ζω0 | Period 2π/ω0 |
|---|---|---|---|---|---|
| press | 28.2 rad/s | 0.65 | 795 | 36.7 | 0.223 s |
| release | 15.8 rad/s | 0.385 | 250 | 12.2 | 0.398 s |

From rest, the press spring peaks at `p` = 1.07 at 0.147 s and is within 2 %
of 1 after 0.23 s. From a settled press, the release spring reaches −0.27 at
0.215 s and +0.07 at 0.43 s, and is within 2 % of 0 after 0.66 s.

- **Growth.** The whole button (silhouette, radii and label) scales
  uniformly about the frame's centre by `s = 1 + 0.245·p` [measured: the
  regular "Glass" button held 1.2444 in width and 1.2468 in height in three
  captures, peaked at 1.263 and fell to 0.932 after release, as the springs
  give]. The recipe keeps the idle `m` (§5.5). Only the regular 66 × 34.33 pt
  button was observed: the same factor at every size and for both kinds is
  [chosen].
- **Wash.** While pressed, a white wash lies over the glass body, below the
  label, composited in encoded space with alpha

  ```
  a = max(p, 0) · 0.30 · (1 − 0.37·u²),   u = min(|x − x_touch| / (w/2), 1)
  ```

  where `x` is the pixel's horizontal position, `x_touch` the touch point's
  and `w` the current, grown width; the wash does not vary vertically
  [measured: the regular button pressed at its centre over black in dark
  appearance went from 31 to 98 at the centre and 73 at the ends, uniform
  within 1 level from top to bottom; fitted: during the press the wash
  follows `p` within 2.7 % of its amplitude, overshoot included]. That the
  wash is centred on the touch point, and not on the frame, is [chosen: only
  centre presses were observed]. During release the wash follows `p` and
  vanishes while `p` is negative [chosen: the release was not captured for
  the wash]. The label keeps its colour while pressed [measured].
- **Drag.** A touch that moves while pressed keeps `p` at 1 and stretches
  the button toward the touch, keeping its area [measured: horizontal drags
  of 20 and 80 pt]:

  | Drag distance | Centre offset toward the touch | Length along the drag | Length across |
  |---|---|---|---|
  | 20 pt | 0.17 pt | ×1.0027 | ×0.9985 |
  | 80 pt | 1.31 pt | ×1.0154 | ×0.9864 |

  The factors are relative to the grown size. Between 0 and 80 pt the
  offset and the factors interpolate linearly through these rows, beyond
  80 pt they stay at the 80 pt row, and they apply along any direction
  [chosen]. They follow the touch through the press spring and return with
  the release spring on release [measured: the centre and the aspect
  returned with the release motion].
- **Leaving the button.** The press held while the touch was 39 pt outside
  the grown silhouette [measured: an 80 pt drag]. The press stays active, and
  a release fires the button's action, while the touch is within 70 pt of
  the hit region; farther out the press cancels: `p` returns by the release
  spring and no action fires [chosen: neither the cancel distance nor the
  action was observed].
- **Setting.** The motion and the wash were observed with the translucency
  setting the device then had, which read 0 before and after; that they do
  not depend on the setting or the appearance is [chosen].

### 8.4 Segmented control

A segmented control selects one of a few options laid out side by side:
a capsule **container** divided into equal **segments**, each holding one
option's label, and a capsule **thumb** under the selected segment. At
rest no part is glass: the container and the thumb are flat fills, with
no shadow and no rim. Glass appears only on the thumb while a touch drags
it (below).

**Reference.** Geometry and motion were observed on the iOS 27.0
simulator (an iPhone 16 Pro, 402 × 874 pt at 3 px/pt) and are tagged
[measured: simulator] or [fitted: simulator]. No device capture was
possible, so the colours below are the simulator's rendering, tagged
[chosen] until a device capture confirms them. Touch interaction was not
observed at all.

#### Geometry

Let the frame be `W` wide with `n` segments.

| Size group | Frame height | Container height | Thumb height |
|---|---|---|---|
| mini, small, regular | 31 | 32 | 28 |
| large, extra large | 48 | 49 | 45 |

All values are [measured: simulator, within 1/3 pt]. Mini, small and
regular are identical, and so are large and extra large.

- **Container.** A capsule `W` wide whose top edge is the frame's top edge;
  it is 1 pt taller than the frame and overhangs its bottom edge by 1 pt,
  which takes no layout space [measured: simulator]. The control takes the
  width it is given.
- **Segments.** The container is divided into `n` segments of width
  `W / n`, whatever their labels: "Short" and "A much longer label" each
  get half [measured: simulator, 2, 3 and 4 segments at 300 and 402 pt].
  No separators are drawn between segments [measured: simulator].
- **Thumb.** A capsule inset 2 pt on every side from the selected
  segment: `W / n − 4` wide and the container's height minus 4 [measured:
  simulator, within 0.05 pt horizontally].
- **Labels.** Each label is centred in its segment on both axes, in 13 pt
  text: regular weight when its segment is not selected and medium weight
  when it is [measured: simulator, ink widths within 1/3 pt of the system
  font's: "Two" 23.33 pt unselected and 24.33 pt selected]. The text size
  is the same at the large size [measured: simulator]. A symbol label is
  drawn at the size of 17 pt text, centred [measured: simulator, a filled
  star inks 18.7 × 17.3 pt; chosen: the 17 pt size, matching that ink].
- **Corners.** The container and the thumb are capsules with continuous
  corners [chosen: the 1/3 pt sampling does not separate circular from
  continuous corners].
- **Direction.** In a right-to-left layout the segments are laid out from
  the right [chosen: not observed].
- **Hit region.** Each segment, extended vertically to at least 44 pt
  about the container's centre [chosen: touch hit-testing was not
  observed].

#### Colours

| Part | Dark appearance | Light appearance |
|---|---|---|
| container | (117, 117, 130) at alpha 0.239 over the backdrop | (115, 115, 123) at alpha 0.122 over the backdrop |
| thumb | (235, 235, 245) at alpha 0.30 over the container | white, opaque |
| labels, selected or not | white | black |

All entries are [chosen: the simulator's rendering, awaiting a device
capture]. Both translucent fills blend in encoded space [fitted:
simulator, the blends reproduce the rendered colours over black, grey 0.5
and white within 1 level: the dark container 28, 125, 222, the light
container 14, 126, 238 and the dark thumb 90, 158, 226 in the red
channel].

#### States

| State | Container and thumb | Labels |
|---|---|---|
| enabled | as above | as above |
| tinted | unchanged: the tint has no effect | unchanged |
| disabled | as enabled, then the whole control at opacity 0.5 | as enabled, at opacity 0.5 |
| dragging | the thumb becomes a glass lens (below) [chosen] | as above |

- **Tint.** A red tint left every pixel unchanged [measured: simulator].
- **Disabled.** The enabled control is composited over the backdrop at
  opacity 0.5, labels included [fitted: simulator, within 1 level: the
  labels 128 over black in dark appearance and 127 over white in light;
  the dark container's alpha halves to 0.122 and the dark thumb's
  combined alpha from 0.467 to 0.235]. A disabled control does not respond
  to touches [chosen: not observed].
- **Focus.** Not observed. An implementation that shows a focused state
  states its own choice.

#### Selection motion

The thumb's drawn position follows a **display position** `s`, the
selected index as a real number; the thumb's leading edge is
`s · W / n + 2`. When the selection changes, `s` moves to the new index by
the **selection spring**, on a unit mass:

| Spring | Natural frequency ω0 | Damping ratio ζ | Stiffness ω0² | Damping 2ζω0 |
|---|---|---|---|---|
| selection | 15.7 rad/s | 0.85 | 246 | 26.7 |

[fitted: simulator, five changes of one and two segments at 60 frames per
second, in both directions; every fit lies within ω0 15.71 to 15.73 rad/s
and ζ 0.850 to 0.851, with a residual below 0.3 % of the travel. A sixth
change, whose recording dropped five frames at the start, is left out.]
From rest, `s` covers 20 % of the change after 0.05 s, 51 % after 0.1 s
and 89 % after 0.2 s, overshoots by 0.6 % at 0.38 s and is within 0.1 %
after 0.6 s.

- **Every change animates.** A selection change moves the thumb by the
  selection spring whether it was made with an animation, with a linear
  1.5 s animation, or with animation explicitly disabled [measured:
  simulator].
- **Thumb.** The thumb keeps its size and colour during the motion
  [measured: simulator, 28 pt tall and of constant width in every frame].
- **Labels.** The labels switch weight at the change, in the first frame
  after it, before the thumb has moved [measured: simulator].
- **Start.** `s` starts at the selection change [chosen: the observed
  motion began 0.019 to 0.032 s after the change was made, which is the
  render pipeline]. A change during the motion starts the spring from the
  current `s` and its velocity [chosen: only changes from rest were
  observed].

#### Touch

Nothing in this subsection was observed: every value is [chosen],
awaiting a touch capture. The values make the thumb a glass lens while it
is dragged, which is the part of the control that shows the material.

- **Tap.** A touch that ends in a segment's hit region without having
  moved more than 10 pt selects that segment at touch-up, with the
  selection motion above [chosen].
- **Press.** A touch that begins on the selected segment drives a press
  progress `p` toward 1 by a spring of ω0 = 28.2 rad/s and ζ = 0.65;
  release or cancellation drives it toward 0 by a spring of
  ω0 = 15.8 rad/s and ζ = 0.385 [chosen: the button's press and release
  springs, §8.1, so that the controls respond alike].
- **Lens.** While pressed, the thumb scales about its centre by
  `1 + 0.15·p`, to about the container's height at the regular size and
  7 pt into each neighbouring segment of a 300 pt, three-segment control,
  and becomes a clear-family glass element (§5.2)
  over the container and the labels: its fill is drawn at opacity
  `1 − max(p, 0)` over the glass [chosen: a lens that shows the labels it
  passes over, kept small enough to stay inside the neighbouring
  segments].
- **Drag.** While pressed, `s` follows the touch: it is the selected
  index plus the touch's horizontal offset divided by `W / n`, clamped to
  [0, n − 1]. At release the segment nearest to `s` becomes selected, and
  `s` settles to it by the selection spring from its position and
  velocity, while `p` returns to 0 [chosen].
- **Leaving the control.** The drag continues wherever the touch goes
  until release; it never cancels [chosen].
