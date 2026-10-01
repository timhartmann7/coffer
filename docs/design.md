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

**The password buttons keep a line of their own.** The mockup gives the row two
buttons and Coffer has four - Show, Copy, Change and Make one - which in a pane
this narrow wrapped anyway, and which line they landed on depended on how long
the value was. On their own line the last of them may take a second one, and it
does so whether or not anything is revealed. Change is the same bordered `h-7`
pill as the others, in words and without an icon, because the mockup's sprite has
no pencil; it reads "Set one" on an entry with no password, and while its field
is open it takes the pressed look Make one takes while the generator is open.

**A revealed value is text, not a field.** The mockup draws it as a line of mono
text, and that is what it is again: selectable, and nothing a key can reach. A
value on one line stays on one line however long it is and scrolls sideways
with no scrollbar drawn, the way the field it used to be shown in did, so
revealing it still moves nothing. A value with line breaks takes the lines it
has, because ten recovery codes on one line are not what the vault holds; a
protected note and a previous version's value wrap their long lines as well. The
field's own countdown now runs under a protected note too. Whatever is revealed
goes the moment the entry comes back changed, however the change arrived: the
password shown before a new one was saved is not the password any more.

**A new value is written in a field of its own.** Change opens, under the row's
buttons, the mockup's field with a text area in mono `small`: one line tall
until the value has more, then growing with it up to twenty lines. A value
already in lines - which Rust says without saying what they are - and a field
the reader named to be written in lines open it four lines tall instead, the
way Notes is, where Return starts a line and Cmd+Return saves. Under it the
two answers in the order every question in the window uses: Cancel, the way out,
as a plain `h-7` pill in `txt3`, then Save as the bordered `h-7` pill beside it.
Neither takes the focus. While the field is empty Save is drawn the way the
password's Copy is on an entry with none - `txt4`, the hairline kept under the
pointer, the not-allowed cursor - and Return does nothing: nothing typed is no
new password. The old value stays where it was, and so does anything shown of
it, until the new one is saved. Save, Cancel and Escape give the focus back to
the Change that opened the field; a field closed because the reader clicked
somewhere else leaves the focus there.

**Leaving that field with something typed asks, in the same place.** The Cancel
and Save line gives way to the window's question box on `surface2`: "Save the
new password?", with Discard as the way out and Save as the neutral pill. There
is no red answer, because neither takes anything out of the vault - a saved
password leaves the old one in Versions, and Discard throws away only what was
typed - and the box does not take the focus, because the reader just put it
somewhere else. Going back into the field takes the question away. The pressed
Change pill, pressed again with something typed, asks the same question.

**The pane stays while that field holds something.** A row pressed, Escape,
the header's close, "+ Entry", a folder chosen, the bin emptied, "Move to
Recycle Bin" or the settings opened over it would each take a new password the
reader just set on a website away with the pane. None of them does anything
then: the pane stays, and the field's question rises again where it is - the
same box, drawn afresh so that it moves where the reader is looking - with the
focus on Save, the answer that loses nothing. Once it is answered, the press
works. While the new value is on its way to the vault the pane waits for it
without asking, since there is nothing left to answer. A state the mockup does
not draw, built from the question it already has.

**A protected field of the reader's own has its Change on the line under it.**
Its row keeps the mockup's shape - name, value, eye - and has no room for a word
more, so "Change" sits under the value, where the value starts, in `fine` and
`txt3` like the Versions block's View and Restore. The row is two columns, the
name's `w-24` and the value's, and Change is in the value's column: it starts
where the value starts because it is in the same column, not because a margin
was worked out to match the name. Pressed, it gives way to the
same field and answers as the password's. A login or an address another client
protected gets the same line under its value. A protected note and a previous
version get a copy beside their eye and no Change: a note Coffer never protects
is read and copied, and a version cannot be written into.

**The step that names a new field offers "Multi-line".** Beside the name, the
generator's own toggle pill - hairline, `meta` mono, the check in `txt2` and
`surface2` behind it when it is on - because it is the same kind of choice. The
format has no such flag, so it decides the fields the value is written in for as
long as the entry is open: a text area four lines tall, where Return starts a
line - the first value's, and a Change's after that.

**A field holding typing that is not written yet carries a dot.** There is no
save button, and a value is written when its field is left, so a note half
written has nothing to say that it is not in the vault yet. From the first key
until the field is left, put back with Escape, or its Change answered, a dot
sits at the end of the field, inside its box: the `1.5` step of the spacing
scale, `rounded-full`, in `warn` - the colour the status bar says "Not saved"
in - with "Not saved yet" under the pointer and for anything that reads the
screen aloud. It is centred on a value on one line and level with the first
line of a value in lines. The mockup draws a finished entry and so no such
state; nothing here is a value it does not contain. What the dot marks is also
what a lock writes before it wipes the window, so the unlock screen after one
says "What you were typing was saved before locking." in the place and the type
of the line about work a lock could not write - `small`, centred, `mt-6` - in
`txt2` rather than `danger`, and names no entry. A new value in a Change field
is written beside the value it was for rather than over it, in a protected
field of its own called after it - "Password (typed before locking)" - where it
is found under Own fields; into the field itself only when that held nothing. A
Change field emptied again has no dot and nothing to write. A box that holds a
name rather than a value - a tag being typed, the name of a new field, a
folder's name - has no dot either: a lock does not write a name the reader has
not finished, so there is nothing for the dot to promise (`docs/ipc.md`, "Only
values are drafted, not names").

**A value of the reader's own in lines is written in lines.** Any value with a
line break is edited in a text area that grows with it, up to twenty lines and
then scrolling, where Return starts a line and Cmd+Return finishes. A field of
the reader's own is one from the start, one line tall and scrolling sideways
until it has a second, so that a paste keeps its breaks.

**The header is the way out of the pane, and nothing else.** The mockup's
header carries one button and it is the trash. Escape had always closed the pane
and nothing else did, and a close put beside that trash - sixteen pixels away,
the same size, the same grey - was a press meant for one landing on the other.
So the header carries the close alone, and deleting is a labelled action at the
foot of the pane, after the versions, where no press aimed at anything else
lands: the trash and "Move to Recycle Bin" in a plain `h-9` pill in `txt3`,
taking `dangerwash` and `danger` under the pointer the way every trash in the
window does. A deletion that would be final - a vault that keeps no bin - reads
"Delete forever…" and asks first.

**An entry is in the pane the moment its row is pressed.** Rust answers one
command at a time, and a save holds it for the second a key derivation takes, so
an entry chosen right after an edit is read only after that save. The mockup
draws no entry that is on its way, and the pane used to go on showing the last
one, versions and all, for that second. Now the heading and the login's box are
drawn at once from the row that was pressed - the name in the title's own type,
the login in the mockup's field, a protected one as the mask - and under them
"Opening…" in `fine` and `txt4`, the colour of something that is not there yet.
The name sits in the title field's own box and the empty ones say "Untitled" and
"No login", the same component and the same words the entry is drawn with. A
row in the bin already says so, and the bin's card stands above the login at
once with its buttons drawn and disabled, so the login does not drop by the
card's height when the entry arrives.
It arrives on the fade, so an entry Rust answers for within the frame replaces
it before it is seen; nothing pulses or spins, because a loop is none of the
window's three movements. Nothing in it can be pressed but the close. The rest
of the entry fills in under the name and the login when it comes, and the pane
fades in only when it opens from nothing, not when it moves from one entry to
the next. The Versions block counts only a list of its own entry's, and has no
number while that list is being read.

**An edit takes the versions off the screen until its save is back.** The save
prunes, and a list drawn through it offered a trash that dropped the neighbour
of the version on its row. So from the moment an edit to the entry lands until
the list read after its save arrives, the block is its heading alone -
"Versions" with no number and no rows, the same state as an entry still being
read - and a question or a version open in it closes. While a restore, a drop
or a clear is on its way, a press on the block is let go. A press Rust still
finds out of date does nothing, and is said in the notice a failure gets, with
the warning in `warn`: "The versions changed while you were choosing, so
nothing was done. Choose again from the list as it is now." The list is read
again under it.

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

**A name in a sentence is set apart from it.** Questions, notices, the bin's
lines and the banners about a lock's copy put the reader's names into running
text: an entry's title, a field's, a file's or a folder's name. Each one is
bidi-isolated - a `<bdi>` in markup, and in a sentence built as a string the
quotes from `quoted()` in `format.ts`, which put the name between U+2068 and
U+2069 and close whatever isolate it leaves open. A name is the reader's or
another client's and may hold right-to-left text or an override; bare, a U+202E
in "This entry already has “…” (1.2 MB)" ran on to the end of the paragraph and
drew the sizes and the warning backwards. Nothing changes on the screen for a
name without one. The mockup draws no such name.

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

**A field whose removal nothing could take back is asked about in its row.**
A field's trash takes it at once, and the notice offers it back - except in a
vault whose limits keep no version for the undo to restore, where Rust refuses
the press. The question then opens under the field's row in the same box as
the others, the trash gone from the row while it is open: "Remove “PIN”? This
vault keeps no version to bring it back from, so this can’t be undone.", with
"Keep it" where the focus lands and "Remove" last in `danger`. After it the
notice reads "Field “PIN” removed forever", without the button, the way an
entry deleted for good does.

**A notice that can be taken back** is the mockup's toast with one thing added:
the trash in `txt3` where the copy icon is, and a button at the end, "Undo" in
the accent with `⌘Z` in the search field's own key cap beside it. The button is
the only part of the notice that takes a press, and the press does not move the
focus, so a field holding typing keeps it. The notice stays eight seconds
rather than five or six, because it is asking for one, and it stays when the
reader opens another entry: the undo is about the thing it names, not about the
pane.

**Edit ▸ Undo in the menu bar is not the notice's.** Cmd+Z reaches the page as a
key before AppKit looks for it in the menu, so the window takes it while an offer
stands and the focus is not in a field. The same item chosen with the pointer
never reaches the page as a key: AppKit sends it to WebKit's own undo, which is
about typing in fields, and the removal stays removed. Making the item the
notice's would mean an Undo of Coffer's own in place of the system's, and that
one would take Cmd+Z away from every field in the window. So the item stays the
system's, the key cap on the notice names the key rather than the menu, and the
button is how the pointer takes a removal back.

**A destructive icon gets room.** The mockup's icons are sixteen pixels with
nothing around them, and a trash twelve pixels from an export or a copy is the
one a press meant for its neighbour lands on. Every trash that removes something
at once - a file's, a field's, a version's - sits in a 28-pixel box (`h-7`),
with `dangerwash` behind it on hover and a step more gap than the icons beside
it; the export next to a file gets the same box with `raised` behind it. The
folder's trash in the header of the folders pane only asks, and gets the same
box all the same: it is the last icon there, a step further from Rename and New
folder, and its box is drawn into the header's padding so the header is no
taller. A version's is drawn into its row's padding, so the row is no taller. A
field's trash is the last thing in the row and is drawn only while the row is
under the pointer or holds the focus, so a row that is being read or copied from
carries no way to lose it.

## The recycle bin

The mockup draws the bin as a row at the foot of the folders and as an empty
state, and nothing of what is in it. Everything below is built from states it
does draw.

**What is in the bin is read, not edited.** An entry opened there is drawn the
way the pane draws a vault Coffer will not write back, every value as text, with
a card at the top of the pane: `surface2` behind a hairline, `rounded-sm`,
`p-3`, the trash in `txt4`, and the sentence in `fine` and `txt2` - "In the
Recycle Bin since 30 Sep · was in “Personal”". Where nothing says where it came
from, the sentence says where it goes instead ("goes back to the top of the
vault") rather than making up a past. Under it "Put back" is the one accent
pill, the unlock button's fill at `h-9`, because putting back is what the card
is for; "Delete forever…" beside it is a plain pill in `txt3` that takes
`dangerwash` and `danger` only under the pointer, and asks in the same card
before anything goes, the red answer last. A vault Coffer will not write back
gets the sentence and no buttons.

**A folder in the bin is a folder.** The bin used to pour a deleted folder out
into itself, its entries mixed in with everything else. The folders in the part
of the bin being shown are now rows above its entries, in the language of the
list they sit at the top of - a band with a hairline under it in the wide list, a
card beside an open entry - with the folder icon at the size of the list's key,
the name in `body` and `txt2`, and the line the narrow list gives a login under
it. Opening one shows what it holds, with the same card at the top of the list
and the folder's name as its heading in `body` and `txt`. The bin's row in the
folders pane stays lit while anything inside the bin is shown, since nothing
else there is.

**A row in the bin says when and where from** - "Deleted 3 days ago · from
“Banking”" - on a second line in `sub` mono `txt4`. Something that went in
with a deleted folder names that folder instead - "Deleted 3 days ago · with
“Banking”" - and its card reads "… · deleted with “Banking” · goes back to
“Personal”", the folder's way back, since its own last move says nothing about
the deletion. A search in the bin looks inside the deleted folders as well, and
that line is how a row it found further down says where it is; the folders'
own rows come back with the empty query. Beside an open entry that
line takes the place of the login and the address. In the wide list it sits
under the name in the first column, which makes those rows taller than the
mockup's rows; nowhere else in the list is.

**A move to the bin is taken back from its notice**, the one described above,
for eight seconds and on Cmd+Z: "Moved “Bank” to the Recycle Bin". A folder
moved there is offered back the same way after its question. A deletion that
was final gets the notice without the button - "Deleted “Bank” forever" -
because nothing can take it back.

**The empty bin tells the truth.** The mockup's sentence promises that deleted
entries stay until the bin is emptied by hand, which stopped being all of it
once one thing could be put back or deleted on its own. It now reads "Anything
moved here waits until it is put back or deleted forever." A folder in the bin
with nothing left in it has an empty state of its own, the folder icon over
the same two lines. Emptying the bin asks in the same box as before, and says
what it is: everything in the bin deleted forever, with nothing to put back
afterwards.

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

The sentence says what is there, in the same box. A vault, or the copy a lock
left of one whose file has gone, gets the "Open it" pill. An empty file a killed
creation left, a folder or a link to nothing gets a sentence that names it and
points at "Somewhere else" in the row above, and no pill: there is nothing there
to open. A press that finds the place changed redraws the box from what is there
now, with the refusal in `danger` at the foot of the form where every other one
on this screen goes.

**The place is written from the tilde.** The creation screen's row shows
`~/Coffer/vault.kdbx` rather than the whole path, which is how the mockup draws
a place (`~/Vault`), and the sentence about a taken place names it the same way.

## Work a lock could not save

The mockup draws no lock at all, so nothing of what one leaves behind. The
unlock screen already had a card for the copy a lock writes beside the vault -
`surface2` behind a hairline, `rounded-sm`, the warn icon and a heading in
`body` - and everything below is that card, the question box, and the
mockup's own warning plane.

**The card offers one way back, and which one depends on the vault's file.**
While the file is there, the card's call is "Open the copy to look" in the
bordered `h-9` pill, because the vault itself can still simply be unlocked and
the accent stays on Unlock. When the file has gone, there is nothing to unlock:
the password field is not drawn, the sentence says the file is not there any
more, and "Put this copy back as my vault" is the accent pill at `h-9`, the way
"Put back" is in the recycle bin's card - on this screen it is the one call
there is. A move that is refused is said under the card in `small` and
`danger`, in words about the move. On a disk that cannot take the copy back
unopened, the card's sentence says the copy opens with the same password and
becomes the vault from inside, and "Open the copy to look" takes the accent
pill's place, because opening it is then the call.

**Removing the copy asks.** "Remove it…" is the plain `txt3` pill that takes
`dangerwash` and `danger` under the pointer, like "Delete forever…", and it
opens the question in the card, under a hairline: "Remove the only copy of those
changes?", with "Keep it" first and focused and "Remove" last in red. When the
vault's file has gone, the copy is the whole vault, and the question is "Remove
the only copy of your vault?" in the same box.

**A lock that could write nothing says what survived.** Under the red line, in
the same `small` and centred, in `txt2`: "Your vault file is as it was today at
14:02.", or that it is not where it was either. On a copy's own screen - the
lock that lost the work was of a copy opened to look - the sentence is about the
copy: "This copy is as it was …". Times of files a reader is comparing are
always said to the minute.

**The copy's own unlock screen says what it is.** The same card, headed "A
copy, not your vault", with when the lock saved it and the vault's file name in
mono, and "Back to my vault" in the bordered pill. Before, only the file panel
led back.

**An open copy says so across the top of the window.** A strip under the title
bar, the full width of the window, on the mockup's warning plane - `warnwash`
behind a `warn/35` hairline at its foot, the plane the creation screen's
password warning is drawn on - with the warn icon. The first line, in `small`
and `txt`, says it is the copy and when it was saved, and that what is changed
here changes the copy and not the vault. The second, in `fine` and `txt2`, says
what "Make this my vault" does to the vault's file: which file it goes over,
when that file last changed - which is how a reader sees that somebody wrote it
after the copy was made - and the snapshot name it is kept under until later
saves push it out of the snapshots, file names in mono. At the end, "Back to my
vault" in the bordered `h-9` pill and "Make this my vault" in the accent pill,
which is what the copy was opened to decide; while it writes, it takes the
Unlock button's busy look and reads "Making it your vault…". A refusal is said
under the second line in `danger`. When the vault's file changed after the
banner said when it last changed, the window reads it again, the second line
says how it stands now, and the refusal says nothing was replaced and points at
that line. The strip is not part of the panes, so the settings do not cover it.

## The settings, and the way in and out of them

The mockup draws the settings as a whole window with a title bar of their own and
draws no way out. Coffer's way in is a button in the status bar, in the corner
the mockup's countdown ticks in — and a settings screen that took the whole
window moved that button to the opposite corner in the moment it was pressed. So
the settings are drawn over the two panes and not over the status bar, and the
button that opened them stays where the hand left it and closes them.

**What is under them is inert, not merely covered.** An opaque sheet hides a
pane; it does not take it out of the tab order, out of hit testing or out of the
accessibility tree. Every value in an entry the database does not protect is a
live field that commits what the reader wrote in it when focus leaves, so a
covered pane is an entry that can be rewritten by keys aimed at something else.

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
stylesheet, once in the script that removes it (`notices.svelte.ts`).
`motion.svelte.test.ts` keeps the two saying the same number.

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
