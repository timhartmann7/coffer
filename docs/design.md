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

## The entry pane

The mockup draws this screen read only: a login is a run of text, a password is a
mask, an address is an underlined line. Coffer's is edited where it stands, and a
value that can be written into is a field. So every value in the pane is drawn in
**the mockup's own field**, drawn in its component sheet next to the buttons and
the list rows: `surface2` behind a hairline, `rounded-sm`, `px-3 py-2.5`, and the
accent ring the moment something in it takes focus. The entry's title is the
exception, because a hairline around a nineteen pixel name is a box around the
name of the screen.

That box is also the fix for the thing this pane was worst at. A masked password
is eight pixels tall and the field that reveals it is forty, so opening an eye
moved everything under it down the screen and closing one moved it back. Both
states now sit in one box, whose height is a fixed line rather than whichever of
the two is showing.

**The countdown is the field's own bottom edge.** The mockup draws it as a row
under the field: the words "Hides in 0:30" and a two-pixel rail. A row that
arrives is a row that moves the rest of the entry, so the rail is drawn along the
bottom inside of the field and the words are shortened to the clock. The sentence
is still there for anything that reads the screen aloud, where three hundred and
eighty-four pixels is not the constraint.

**The three password buttons keep a line of their own.** The mockup gives the row
two buttons and Coffer has three, which in a pane this narrow wrapped anyway —
and which line they landed on depended on how long the value was.

**There is a way out of the pane.** The mockup's header carries one button and it
is the trash. Escape has always closed the pane and nothing else did, so the
close goes beside the trash rather than after it: the mockup's own rule is that
the destructive action stands at the end of a row, so that missing it costs a
movement.

## Asking before, and taking back after

The mockup draws one choice, the conflict dialog, and states its rule beside it:
outcomes named in words and by their result, the destructive one last and the
only one in red. Everything the window asks before something goes follows that
rule, in one component, `Confirm.svelte`, and nothing asks any other way.

**The question is asked where the press landed.** A box of the other plane from
the one around it (`surface` in the folders, `surface2` in the entry pane)
behind a hairline, `rounded-sm`, `p-3`, the question in `fine` and `txt2`. Under
it the way out as a plain `h-9` pill in `txt3`, named by what it leaves ("Keep
it"); a neutral step, when there is one, in the bordered pill the window uses
for any action that is not the page's one call ("Save a copy first…"); and the
destructive answer last, in `danger` over `dangerwash`. Asked inside something
that is already a card - a file's row, a version's - it drops its own box and
sits under a hairline in the card. Clearing the history used to ask on one line
without a box; it asks in the box now, like the rest.

**A file whose name is taken is asked about in that box,** under the
Attachments label and the plus that asked for the file: "This entry already has
“Scanned Document.pdf” (1.2 MB)", what the new one would be called, and that a
replacement cannot be undone. It is the one question whose focus is not on the
way out. "Keep both" is the neutral pill in the middle and it is where the focus
lands, so Return gives it, because it is what somebody adding the second page
of a scan means; "Don’t add it" is the way out and Escape still gives it, and
"Replace" stands last in `danger`. When a replacement is refused because earlier
versions hold the file there, the same box asks again with the sentence saying
why and without "Replace": the same keys, one button fewer. Once the file there
has gone - its own trash is the way the sentence points to - the box says so and
offers "Add it" in the same place. The banner that used to answer a refusal
ended in a button called "Right you are", with the chosen file already gone.

**A notice that can be taken back** is the mockup's toast with one thing added:
the trash in `txt3` where the copy icon is, and a button at the end, "Undo" in
the accent with `⌘Z` in the search field's own key cap beside it. The button is
the only part of the notice that takes a press, and the notice stays eight
seconds rather than five or six, because it is asking for one.

**A destructive icon gets room.** The mockup's icons are sixteen pixels with
nothing around them, and a trash twelve pixels from an export or a copy is the
one a press meant for its neighbour lands on. Every trash that removes something
at once - a file's, a field's, a version's - sits in a 28-pixel box (`h-7`),
with `dangerwash` behind it on hover and a step more gap than the icons beside
it; the export next to a file gets the same box with `raised` behind it. A
version's is drawn into its row's padding, so the row is no taller. A field's
trash is the last thing in the row and is drawn only while the row is under the
pointer or holds the focus, so a row that is being read or copied from carries
no way to lose it.

## A vault that is already there

The mockup's first run is for somebody who has nothing, and its creation screen
assumes the place is free. Two states answer the reader who already has a vault
on this Mac, and both are built from states the mockup does draw.

**The first run says what it found.** Above "Make a vault" sits the card the
unlock screen already offers a lock's rescue copy in: `surface2` behind a
hairline, `rounded-sm`, the disk icon in `txt4`, the sentence in `body` and the
file name in mono, and the same bordered `h-9` pill for "Open it". It is not the
accent: the accent stays on the screen's one call to action, and a reader who
presses it anyway is told on the next screen that the vault is there.

**The creation screen says the place is taken** under the row that names it,
before a password is typed: the corrupted-file state's warn icon and its bordered
pill, with the sentence in `small` rather than `lead`, because the row above it is
the thing it is about. The make button goes to its disabled look until the place
is free.

**The place is written from the tilde.** The creation screen's row shows
`~/Coffer/vault.kdbx` rather than the whole path, which is how the mockup draws
a place (`~/Vault`), and the sentence about a taken place names it the same way.

## The settings, and the way in and out of them

The mockup draws the settings as a whole window with a title bar of their own and
draws no way out. Coffer's way in is a button in the status bar, in the corner
the mockup's countdown ticks in — and a settings screen that took the whole
window moved that button to the opposite corner in the moment it was pressed. So
the settings are drawn over the two panes and not over the status bar, and the
button that opened them stays where the hand left it and closes them.

**What is under them is inert, not merely covered.** An opaque sheet hides a
pane; it does not take it out of the tab order, out of hit testing or out of the
accessibility tree. Every value in an entry is a live field that commits what is
in it when focus leaves, so a covered pane is an entry that can be rewritten by
keys aimed at something else.

## Motion

The mockup is a still picture and names no duration. The window has three, and
nothing defines a fourth:

- **a rise**, 170ms, for something that came from below or grew out of a press: a
  notice, a generator, a list of versions, a name being typed;
- **a fade**, 130ms, for something already the size it is that only had to
  appear: a pane, a dialog's veil, an empty state;
- **a pop**, 120ms, for a list that belongs to the control under it, growing out
  of that control's edge. The dropdown on the settings screen is the only one.

None is longer than a fifth of a second, and every one is silenced by reduced
motion — which is the whole difference between them and the two drains above.

**A pane opens by its column and not by itself.** The entry pane used to be a
column that existed or did not, so opening an entry relaid the whole screen out
between two frames. The grid keeps three tracks either way and the last one is
what opens and closes; the pane inside keeps its own width and is clipped by the
track, so an entry is never laid out at a width nobody asked for on its way in.

**A notice leaves on a transition, which is the one thing a keyframe cannot
do.** Nothing animates an element that has already been taken out of the
document, so a notice that is going stays until it has gone. That is the only
motion in the window whose length is written down twice — once in the
stylesheet, once in the window that removes it. `motion.svelte.test.ts` keeps
the two saying the same number.

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

## The list beside an open entry

The mockup gives this list the wide one's rule: a hairline under every row, and
a plane behind the one being read. At two hundred and thirty pixels that reads
as one block of stripes rather than as a list of entries, which is the width the
mockup draws it at and the width it is wrong at.

So it takes the folder list's language instead - the same window's answer to the
same question at the same width. A card with a hairline that appears under the
pointer, the raised plane and an accent bar down the side of the one being read,
and the key or clip the wide list marks a row with. Nothing here is a value the
mockup does not contain; what changed is which of its own patterns this list
belongs to.

## The list row

The mockup draws a row as a `div` that lights up under the pointer, and says
nothing about what a press on one does, because nothing in a still picture does
anything. Coffer's row is a `button` with the grid inside it, so that the whole
of what lights up is the whole of what opens the entry. The two copy buttons at
the end are laid over the column the mockup leaves empty for them, because a
button cannot hold a button.

## One size that was off the scale

The tag's remove cross was 12 pixels where the mockup's smallest icon is 14.
