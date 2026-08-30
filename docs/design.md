# Where the window departs from the mockup

`design.html` is the source of truth for everything visual, and it draws one
theme on a machine with a window server. Some of what a shipped application
needs is not a decision it makes. This is the list, so that the next reader can
tell a departure from a mistake.

Everything here is built from the mockup's own tokens. Nothing invents a value
except where it says so.

## The light theme

The mockup has no light theme. This one is the same palette read from the other
end: every colour keeps its hue and its chroma and takes the lightness its dark
counterpart has, counted down from the top of the ramp instead of up from the
bottom.

Two things that rule does not settle on its own.

**Canvas does not invert with the planes.** It is not a plane here: the window is
the surface, and canvas is only the ink an accent fill carries and the veil over
a dimmed window. Mirroring it to near-white is what keeps both readable.

**The accent lands on `#4e58b8`, which the mockup already names** as `accentdim`
in its token list. So the light theme adds exactly one colour the mockup does not
contain: `--color-accenthi: #3e44a3`, the hover, which is the dark theme's own
hover step of 1.325 taken in the other direction.

The light ramp is anchored to clear 4.5 : 1 on `raised`, the deepest plane the
window paints text on. The dark ramp, which is the mockup's, clears it on the
window plane and falls to 4.28 on the chrome and 3.90 on a selected row. Those
are not ours to retune; the light theme is, so it clears the bar everywhere. The
side effect is that light text reads a little heavier than dark, and
`theme.test.ts` records the measurements either way.

`--color-warn` is the one colour that changes character rather than lightness.
Amber cannot be readable on a light plane and stay amber, so in light it is a
dark ochre. Every use of it is an icon or a border, never text.

## The controls

**The theme row's segmented pill** draws its hover and its pressed state, which
the mockup does not: it draws three static spans. Derived from the same tokens,
the way the switch derives the off state.

**The generator's length slider** takes the mockup's rail and its knob — a
three-pixel `line` rail, a sixteen-pixel `txt` knob with a hairline around it —
and does not take its filled portion. WebKit exposes no pseudo-element for the
fill, and the only other way to draw it is a width computed into a style
attribute, which this window's `style-src 'self'` throws away. The number beside
the label is what says where the handle is.

**The two draining bars are exempt from the reduced-motion rule.** The mockup
carries the blanket, but the mockup is a still picture. A bar whose length is a
countdown emptying at once beside "Hides in 0:30" would be saying something that
is not true, and a rail crossing a screen over half a minute is not the motion
that preference is about.

## The application icon

The mockup draws the icon as a tile: a rounded square filled `canvas`, with the
mark at two thirds of it. That is exactly what the brand file holds. What the
mockup does not address, because a web page has no reason to, is the transparent
margin macOS expects around it.

**The tile is drawn into the middle 824 of 1024.** macOS masks nothing and scales
an icon to whatever size the file gives it, so a tile drawn to the edge stands a
quarter wider than everything beside it in the Dock. The number is Apple's grid,
measured off the icons already on this machine rather than recited.

**There is no drop shadow.** Apple's own icons carry one; adding it would be
inventing a visual language the mockup does not draw. It is a few lines of the
rasteriser away if it is ever wanted.

**Below 28 pixels the halftone merges,** which `design.html` says itself: the mark
is 391 dots at a pitch of 22.7 units, and at the 16-pixel layer that pitch is
about a third of a pixel, so the dots blend into a flat mix of the accent over
the ground. The mockup calls for a simplified solid variant at those sizes. It
does not exist yet, and drawing one is not this slice. The Dock draws 128 by
default, where the halftone reads clearly.

**The disk image has no background picture.** The mockup draws no disk image, and
the bundler's defaults already put the application and the Applications alias
where a reader expects them.

## Two sizes that were off the scale

`min-w-[9rem]` on the password row became `min-w-36`, the same length as a step
the scale already has. The tag's remove cross was 12 pixels where the mockup's
smallest icon is 14.
