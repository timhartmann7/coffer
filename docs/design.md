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
the label is what says where the handle is. With digits and nothing else chosen
the rail runs from four rather than eight and its label reads "PIN length", and
the look-alike switch is not drawn: a PIN keeps 0 and 1, having no letter in it
to take them for. The password's generator and a field's remember their own
settings, so a PIN made for a card does not become the next password.

**The generator says what its switches mean.** The mockup's "!@#$%" switch reads
"Symbols", and under the switches, in `label` mono `txt4`, the thirty-two
characters it draws from are written out. The mockup's "Look-alike characters"
switch reads "Avoid look-alikes (0 O 1 l I |)" and is on to begin with, which is
what the generator always did with it off. Under them, in the length's own
label, "Avoid these characters" and the mockup's field in mono `small`, for the
quote and the backslash a bank turns away. The footnote beside "Put it in the
field" gives way, in `warn`, to "No digit in this one." and a line saying to
make another when a password lacks a kind that was asked for: it stands where
the reader decides, and takes no room the footnote did not.

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
new password?", with "Keep typing" as the way out, Save as the neutral pill and
Discard last, as the red answer. The way out, and so Escape, goes back into the
field and loses nothing: Escape is what a reader presses again when the pane
did not close, and a way out that discarded threw away a password just set on
a website on that second press, with nothing said. Discard takes nothing out of
the vault, but it throws away the one copy of what was typed, which is why it
is the red one and never what a key gives. The box does not take the focus
when the reader put it somewhere else. Going back into the field takes the
question away. The pressed Change pill, pressed again with something typed,
asks the same question.

**The pane stays while that field holds something.** A row pressed, Escape,
the header's close, "+ Entry", a folder chosen, the bin emptied, "Move to
Recycle Bin" or the settings opened over it would each take a new password the
reader just set on a website away with the pane. None of them does anything
then: the pane stays, and the field's question rises again where it is - the
same box, drawn afresh so that it moves where the reader is looking - with the
focus on Save, the answer that loses nothing. Once it is answered, the press
works. While the new value is on its way to the vault the pane waits for it
without asking, since there is nothing left to answer. A state the mockup does
not draw, built from the question it already has. A move is not on that list:
the entry stays in the pane wherever it goes, and so does the field.

**A protected field of the reader's own has its Change on the line under it.**
Its row keeps the mockup's shape - name, value, eye - and has no room for a word
more, so "Change" sits under the value, where the value starts, in `fine` and
`txt3` like the Versions block's View and Restore. The row is two columns, the
name's `w-24` and the value's, and Change is in the value's column: it starts
where the value starts because it is in the same column, not because a margin
was worked out to match the name. Pressed, it gives way to the
same field and answers as the password's. A login, an address or a note another
client protected gets the same line under its value - the note's Change written
in lines - and so does a title: a database that protects titles draws the
heading as the mask, and a "Title" row above the login, in the login's label
and field, reads, copies and changes it. An empty protected title is typed into
the heading like any other. A previous version gets a copy beside its eye and no
Change, because a version cannot be written into.

**A field of the reader's own carries the password's tools.** A hidden one has
"Make one" beside its Change, in the same `fine` `txt3` words, and pressing it
opens the generator under the row at the width of the pane, the way the
password's does; it reads `txt` while the generator is open, the pressed look
the password's Make one takes. An empty hidden field, which is typed into where
it stands, has Make one alone on that line. Every value in the pane that is not
empty has a copy - the address after its opener, the notes level with their
first line, a field of the reader's own after its value - the mockup's
sixteen-pixel copy in `txt4`, where the login's stands. In the name's column,
before the name, every field of the reader's own carries the sprite's lock -
`lock` while the value is hidden, `unlock` while it is kept in the open - in
`txt4` at the size of the toggle pills' check, pressed to change which, and
drawn whether or not the row is under the pointer, because on an empty field it
is the only thing that says which the value will be. It stands there rather
than in the value's column, where it took the width a revealed value is read
in. The mockup's row has an eye and nothing else; the copy and the lock are
additions of the same kind as the eye, in its colour.

**A field's name is pressed to be renamed.** The name in its `w-24` column is a
button in the same `small` `txt2`, `txt` under the pointer, with the whole name
in its tooltip, because the column truncates. Pressed, the row gives way to the
field the name of a new field is typed in - `border-accent`, the accent ring,
`surface2`, `small` - across both columns, holding the name selected. Return or
leaving it renames; Escape puts it back. Fields of the reader's own are drawn
in the order a reader looks for them, "Code 2" before "Code 10" and "pin"
beside "PIN", which the mockup's two rows say nothing about.

**The step that names a new field offers "Hidden" and "Multi-line".** Beside
the name, the generator's own toggle pill - hairline, `meta` mono, the check in
`txt2` and `surface2` behind it when it is on - because it is the same kind of
choice. "Hidden" is on to begin with, and decides whether the value is kept
protected; the lock beside the field changes it later. The format has no
multi-line flag, so "Multi-line" decides the fields the value is written in for
as long as the entry is open: a text area four lines tall, where Return starts a
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
is found under Own fields; into the field itself only when that held nothing.
A lock that kept one so adds a second sentence to the same line, naming no
entry and no field either: "A new value you had not saved yet was kept in a
field of its own, beside the old one, which is unchanged." A
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

**The header is the way out of the pane, and to a copy of it.** The mockup's
header carries one button and it is the trash. Escape had always closed the pane
and nothing else did, and a close put beside that trash - sixteen pixels away,
the same size, the same grey - was a press meant for one landing on the other.
So deleting is a labelled action at the foot of the pane, after the versions,
where no press aimed at anything else lands: the trash and "Move to Recycle Bin"
in a plain `h-9` pill in `txt3`, taking `dangerwash` and `danger` under the
pointer the way every trash in the window does. A deletion that would be final -
a vault that keeps no bin - reads "Delete forever…" and asks first. Beside the
close stands "Duplicate", in words, as the password's bordered `h-7` pill in
`fine` `txt2`, with "Duplicate · ⌘D" under the pointer the way the list's copy
buttons name their keys. A copy may sit where a deletion may not: a press meant
for the close that lands on it loses nothing, and what it made is put away like
any other entry. It is drawn only for an entry Coffer can write and that is not
in the bin, and an entry still being read draws it greyed - `txt4`, the hairline
kept under the pointer, the not-allowed cursor - so the name beside it does not
narrow when the entry arrives.

**An entry just made opens with its name selected.** A kind, a template or a
copy is made, opened, and its title takes the focus with all of it selected, so
the first key types its name: "Gmail copy" is there to be typed over, and an
empty one to be typed into. Only when the focus is nowhere: a reader who went
on to type somewhere while Rust was making it keeps typing there. A name the
database protects is drawn as the mask and takes nothing. No notice says a copy
was made: the copy in the pane says it.

**The step that names a new field suggests names.** Under the name and its two
pills, a line: "Suggested" in the mono label, and a pill for each of PIN,
Account number and Security answer - the generator's toggle pill, `meta` mono
`txt3`, with the sprite's `plus` in `txt4` at the check's size - for those the
entry does not have and whose name starts with what is typed, in either case.
A press makes the field there and then, hidden as the suggestion says, and its
value takes the focus; the line goes when nothing is left to suggest. The pills
take no focus from the name, so pressing one is not leaving it.

**A field an entry's kind writes in lines is a text area from the start.** A
licence key, recovery codes and a secure note's secret are made empty, and an
empty value has no line break to say it is in lines, so the fields the kind
names open four lines tall, the way a field named with "Multi-line" does, for as
long as the entry is open.

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
read - and a question or a version open in it closes. A list asked for before
the latest such edit is dropped when it answers, since saves overlap and it can
answer after that edit. While a restore, a drop or a clear is on its way, a
press on the block is let go. A press Rust still finds out of date does
nothing, and is said in the notice a failure gets, with the warning in `warn`:
"The versions changed while you were choosing, so nothing was done. Choose
again from the list as it is now." The list is read again under it.

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
U+2069, close whatever isolate it leaves open, and show a paragraph separator
in it (a line feed, U+2029 and the others UAX #9 counts) as a space, since one
ends the isolate along with the paragraph. A name is the reader's or
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

## Moving things

The mockup draws a folder line above an entry's title and a tree of folders,
and nothing that moves an entry or a folder between them. Everything below is
built from what it does draw.

**The line above the title is where the entry is, and where it goes.** The
mockup writes the folders down to an entry above its title in `label` mono
`txt4`. In an entry Coffer can write, the same line is a button in the same
type, `txt2` under the pointer, with the sprite's `chev-d` at the settings
chip's size after it, and it reads "Top of the vault" for an entry in no folder,
where the line used to be left out. Each folder's name in it is isolated, so a
right-to-left name turns nothing round but itself. An entry still being read
draws the same line with nothing to press, so nothing moves when it arrives; an
entry Coffer will not write, and one in the bin, keep the plain line.

**The folder list is the settings chip's list with a filter on top.**
`bg-raised` behind a hairline, `rounded-sm`, arriving on the pop: a heading in
`label` mono `txt3` ("Move to", "Put it in"), the field a new folder's name is
typed in with "Find a folder", and the places in the tree's order, each a line
of the tree's height - the folder icon, the name in `small`, the folders above
it after it in `meta` mono `txt4`, each name isolated as the line above the
title isolates them - with the top of the vault first under the sprite's disk.
Nothing in the bin is listed. Where the thing is now has its icon in the
accent, the way the tree marks the folder being shown; the line the keys are on
is `surface2`, as the chip's chosen value is, and Return takes it. The list
scrolls past `h-40`, and keeps the line the keys are on in sight, the one it
opens on included, so Return never takes a line the reader was not shown. A
folder chosen from it gives the focus back to the line above the title, which
stays wherever the entry goes. Under the title it is the header's width; under
"+ Entry" it is the notice's 292 pixels. "No folder matches “…”" in `fine`
`txt4` when nothing does. Escape, and the focus leaving it, close it with
nothing moved, and Escape goes no further: the search and the pane stay.

**A row or a folder is dragged with the pointer, not by the system.** Nothing is
drawn under the pointer: an image that followed it would be a position written
into a style, which `style-src 'self'` forbids, and a system drag would put the
row on the drag pasteboard, where whatever application is under the pointer
reads it. A press that travels four pixels lifts the row, which keeps the
`raised` plane, the cursor is the closed hand, and the folder it would land in
is drawn the way the tree draws the folder being shown, with the accent outline
the focus ring is - two pixels, inside the line so the pane does not clip it.
Only a place that takes it lights: not where it already is, not itself or a
folder under it, nothing in the bin. "All entries" and "Not in a folder" are
the top of the vault, and each lights on its own. A folded folder opens when the
pointer rests on it for 0.7 s. Escape lets go with nothing moved, and the
release that follows presses nothing under it. The tree does
not scroll by itself under a drag; a folder out of sight is reached through the
line above the title.

**A move is said, and taken back, in the notice.** "Moved “Chase” to
“Banking”", "Moved “Home” to the top of the vault", with the folder icon in
`txt3` where a deletion's notice has the trash, and Undo and ⌘Z as for every
offer. Undo puts each thing back in the folder it came from, at the end of it,
for a folder as for an entry. Once the file no longer holds the move - another
move since, or a file read again in which it never happened or something was
moved on, gone or deleted - the undo moves nothing and says what every undo
that came too late says, "Something has changed since, so that can no longer be
undone." A file read again that still holds the move takes it back as before.

**An empty folder says how entries get into it.** The mockup's sentence, "Entries
can be made here or dragged in from other folders.", now that both are there to
do. A vault Coffer will not write back offers neither, and says "Entries live in
folders. This one has none of its own." instead, with no button.

**"Not in a folder" lists what sits at the top of the vault.** Under "All
entries", in its row's shape, with the disk icon and the count, drawn in a vault
with folders while the top holds entries of its own, and while it is chosen.
Its list is only those entries, and so is a search in it. Chosen and empty: the
disk icon over "Every entry is in a folder" and "Entries at the top of the
vault, outside every folder, show up here." It is no folder, so the folders
header offers no rename and no deletion on it.

**"+ Entry" in "All entries" asks where.** In a vault with folders it opens the
list as "Put it in" under the button, on the folder the last entry this window
made went into, so Return makes it there - and on the top of the vault when this
window has made none. A lock forgets it, as it forgets everything of the vault
(`docs/ipc.md`). In a folder, in "Not in a folder", and in a vault with no
folders, the entry is made there with no question. A kind or a template chosen
from the button's own list asks the same way (see "Making an entry").

## Making an entry

The mockup draws "+ Entry" as one pill that makes a login. A reader keeps cards,
networks, passports and codes as well, and Coffer makes each with the fields it
needs (`docs/vault-core.md`, "Kinds, templates and copies").

**"+ Entry" is split in two.** The mockup's pill, its hairline and `h-9` kept,
holds two buttons: "+ Entry", which makes a login as it always did, and after a
hairline the sprite's `chev-d` in `txt4` at the settings chip's size, read out
as "Other kinds of entry". Each half takes the press on its own,
`active:bg-raised` on its own side of the pill.

**The chevron opens a list of every kind, and then the vault's templates.** The
settings chip's list: `bg-raised` behind a hairline, `rounded-sm`, arriving on
the pop under the pill's right edge, a line per kind in `fine` `txt2`,
`surface2` and `txt` under the pointer and on the line the keys are on. In a
vault whose file names a templates group, a hairline and a heading - "Templates
in this vault" in the mono label, `txt4` - and a line for each template, its
title isolated, the mask for a title the database protects and "Untitled" in
`txt4` for none, cut at thirty-four characters, the width of an empty state's
sentence. The list scrolls past 420 pixels, a height the mockup already draws
(the window behind its conflict dialog), so a vault with a hundred templates
does not run it off the window. Opening it puts the focus on its first line -
the arrows on the chevron open it on the first or the last - the arrows step
through it and round, Escape closes it with the focus back on the chevron and
goes no further, from a line or from the chevron, a second press on the chevron
closes it with the focus there, and the focus leaving the control closes it. A
line chosen makes that kind or that template, asking where first in "All
entries" as "+ Entry" does.

**An empty vault offers every kind at once.** In place of "Add an entry" under
"This vault has nothing in it yet", one pill per kind, the password's `h-7` pill
in `fine` `txt2`, wrapping at forty-six characters, the width of the empty
state's title above them, and centred: the first thing somebody keeps is as
often a card or the Wi-Fi as a login. An empty folder keeps its one "Add an
entry", which makes a login; a vault Coffer will not write offers neither.

## A vault that is already there

The mockup's first run is for somebody who has nothing, and its creation screen
assumes the place is free. Two states answer the reader who already has a vault
on this Mac, and both are built from states the mockup does draw.

**The first run says what it found.** Above "Make a vault" sits the card the
unlock screen already offers a lock's rescue copy in: `surface2` behind a
hairline, `rounded-sm`, the disk icon in `txt4`, the sentence in `body` and the
file name in mono, and the same bordered `h-9` pill for "Open it". It is not the
accent: the accent stays on the screen's one call to action, and a reader who
presses it anyway is told on the next screen that the vault is there. When what
was found is the copy a lock left of a vault whose file has gone, the sentence
says it found a copy of that vault, kept when it last locked, that the vault's
own file is not there, and that opening it is how the copy goes back.

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
on this screen goes. So does coming back to the window while the place is not
free: a reader sent to the Finder to clear it returns to a box that says what
they left there, and to a make button that is ready once they have.

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
the only copy of your vault?" in the same box. When the chosen file is itself a
copy a lock left - the reader was in one, and the lock that closed it kept
their work in a copy of that copy - it is that copy's file that went, not the
vault's: the card says "This copy is not there any more", and the question is
"Remove the only copy of what this copy held?".

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

## Looking at a backup

The mockup draws no backup, and nothing of a vault Coffer will not write.
Everything below is the copy's strip and card, the settings' own rows, the
generator's small pills, the dropdown's card and the notice's.

**A backup that is open says so across the top of the window.** The strip an
open copy wears - `warnwash` behind a `warn/35` hairline at its foot, the warn
icon - is one component now, `Strip.svelte`, for both. The first line, in
`small` and `txt`, says it is a backup and the moment it holds the vault as, to
the minute, and why it is open: the vault's own file could not be opened, or is
not there any more, or, when the reader opened it to look, that nothing in it
can be changed. The second, in `fine` and `txt2`, says what "Use this copy as my
vault" does to the vault's file: which file it goes in place of and when that
file last changed, that a file which opens with this backup's password is kept
as the newest backup until later saves push it out and one that does not is kept
beside it under a name of its own, and that nothing is deleted - or, when the
vault's file has gone, that the backup goes back as it. Both end on the password: from then on, the vault opens
with the one this backup opens with, which after a change of the master password
is the old one, and a reader who changed it because it leaked hears that before
the press. File names are in mono. At the end, "Back to my vault" and "Save a
copy as…" in the bordered `h-9` pill and "Use this copy as my vault" in the
accent pill, which takes the Unlock button's busy look and reads "Making it your
vault…". A refusal is said under the second line in `danger` - a copy aimed at a
name that holds a file among them, in Rust's words, since a copy never replaces
one - and a copy that was kept, where it went, in `txt2`. What became of the
replaced file is said afterwards in the notice, with the toast's copy icon, the
way "Kept as" is.

**What a lock took the window from saying, its unlock screen says.** Two lines
in the place and type of the one about a password changed before locking -
centred, `small`, `txt2`. A backup made the vault and a lock that landed before
its notice: the vault was made from a backup, it opens with the password that
backup opened with, and the replaced file's name. A backup asked for from the
settings that the lock's own save pushed out of the chain on the way: that it
is not there any more, why, and that this is the vault's screen.

**A chosen backup's unlock screen says what it is.** The copy's card, headed "A
backup, not your vault": the vault's file name in mono, the moment the backup
holds it as, the password it opens with - the one the vault had then - and "Back
to my vault" in the bordered pill.

**Backups are a list wherever they are offered.** A row is the moment in `small`
and `txt` ("Today, 14:05", "27 Aug, 18:40"), to the minute because ten saves an
hour apart are told from each other by nothing else, the file name under it in
mono `meta` `txt4`, isolated, and the action at the end in the bordered `h-7`
pill the generator and the Change field use. Rows are divided by `line`. A
filesystem that keeps no times says "Time not kept" in `txt3`. In the settings,
"Automatic backups" takes the place of the row that named the snapshots' file
names: how many there are in mono `fine` `txt3`, and "Show" in the bordered pill
"Open another" wears, which opens the rows under it inside the same divided
list, at the rows' own `px-8`, each with "Open to look", and "Open now" in `fine`
`txt4` for the one already open. On the screen for a vault file that will not
open, or is not there, the same rows sit in the rescue card's plane (`surface2`,
a hairline, `rounded-sm`) under a line in `small` and `txt2` that says each opens
with the password the vault had then, each with "Open" - all but the chosen
file itself, when it is a backup that would not open either. A backup that went
before it was pressed is said under the list in `small` `danger`, and the list
is read again; one read again with nothing in it says "No backups are left." in
`small` `txt3` above that line, rather than taking it away with the list. The
settings read the list again after the password row moved it: a new master
password is a save, and old backups removed are gone.

**Read only is a button, and its note says why.** The status bar's word, in the
status bar's own type, opens a note above it on the pop: the dropdown's card -
`raised` behind a hairline, `rounded-sm` - at the notice's width, `w-[292px]`,
with the reason in `fine` `txt2` and, where a copy can be written somewhere
else, "Save a copy somewhere else…" in the small bordered pill. The note sits
inside the status bar, which spaces its letters and draws in mono capitals, so
it puts back the sans face, the case and the spacing every sentence in the
mockup has; the spacing is the one value here the mockup does not name, because
it never had to: `--tracking-normal`, zero, the absence of one. It closes on
Escape, which goes no further, when the focus leaves it, and when its button is
pressed again; a notice rising in the corner is drawn over it. WebKit gives a
button no focus when it is clicked, so the note puts the focus on its button
when it opens, and its copy button keeps the focus where it is on a press:
otherwise a note opened by a pointer would hear neither Escape nor a press
elsewhere, and Escape would close the entry under it.

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

## Changing the master password

The mockup's settings draw no row for it, and its creation screen is the only
place it draws a password typed twice; the row is built from both. `SPEC.md`
names four things for this screen, and key derivation still does not appear.

**A row like the vault's, with the creation screen's fields under it.** "Master
password" is in `row` and `txt`, with "What you type to unlock this vault -
nothing, if its key file alone opens it" under it in `fine` and `txt3`: true of
a password alone, of a password beside a key file, and of a key file alone,
whose reader would otherwise have nothing telling them the current password is
an empty field. "Change…" is the bordered `h-9` pill "Open another" is. The row
comes right after the vault's, and is drawn only while a vault Coffer can write
is open - and not inside the copy a lock left, which can be written and is not
the vault: a password given there would be the copy's alone, while the vault and
every snapshot of it went on opening with the old one. The copy's banner is the
way to make it the vault, and the row is there from then on. With such a copy
beside the vault, the row is drawn without the pill, and a line under it in
`small` and `txt2` says why and what to do: "Not while the copy a lock left sits
beside this vault, which opens with the same password. Lock the vault, then make
the copy your vault or remove it, and the password can be changed here." The
unlock screen's card about the copy is where both are done. Pressed, the pill
gives way to three fields under the row, with the cursor in the first. "Current
password" is on a line of its own, at the width of one column; "New password"
and "Once more" sit side by side. They use the creation screen's field, its mono
label and its two-column grid with a `5` gap. Under them is the creation
screen's warning, the same component at the same size, because the new password
is as unrecoverable as the first. Then come "Never mind", the plain `h-9` pill
in `txt3`, and "Change the password", the accent pill at `h-9` that "Put back"
is. While Rust works the button reads "Changing it…", both buttons are disabled
and every field is read only. Return in the first two fields moves to the next,
and sends from the third.

**Every field has the mockup's eye.** The mockup draws one at the end of the
unlock field and of the creation screen's first field, in `txt3` at sixteen
pixels, and the check alone at the end of the creation screen's "Once more".
Pressed, the field shows what is typed, in mono, until the focus leaves the
field and its eye; then it is a mask again, so a password is never left on the
screen by a reader who went to type somewhere else. Pressing the eye leaves the
focus in the field. Its name is what it does next, "Show the password" or "Hide
the password", as the eye on a revealed value is named, and it is not also a
pressed toggle. Here "Once more" has an eye as well as the check, so that each
of the three can be read back before it is sent. "Once more" draws the mockup's
check before its eye, in the same `txt3`, once the two new passwords are the
same. They are compared when the focus leaves either, not on every key, and a
key in either takes the check away. The creation screen's two fields are this
same field drawn without the eye, so its look and its wrong state live in one
component; that screen still draws neither the eye nor the check the mockup
gives it.

**Refusals are said under the form, in `small` and `danger`.** Two new passwords
that differ give the creation screen's sentence; only "Once more" is emptied,
with the cursor in it, and nothing is sent. An empty one gives the creation
screen's sentence for that. Anything Rust refuses begins "The password was not
changed:" and keeps everything typed. A wrong current password paints that
field's border `danger/60`, the mockup's wrong-password state, and puts the
cursor back in it with its text selected; a new one Rust refuses puts the cursor
in "New password". A file somebody else wrote stops the change the way it stops
a save, and the conflict dialog rises over the settings; a file that is not
there any more raises the dialog a save raises for that. Each sentence arrives
in a live region that was there before it, so a screen reader says it wherever
the focus is, and "Once more" is described by the sentence about the two while
it stands.

**The settings stay while the change is on its way.** Escape and the status
bar's button do nothing until Rust answers, because the answer is the one place
that says which password now opens the vault. Otherwise Escape in the form puts
the form away, with what was typed, and leaves the settings open with the focus
on "Change…".

**Afterwards, the snapshots are asked about in the window's one question box,**
on `surface2` under the row: "Master password changed. Your 4 automatic backups
of earlier saves still open with the old password until later saves replace
them. Removing them can’t be undone." One is said in the singular, with "Keep
it" and "Remove the old backup". "Keep them" is the way out and where the focus
lands, and Escape is Keep; "Remove old backups" is last, in `danger`. Closing
the settings keeps them. A line in `fine` and `txt3` under the row then says how
many went, in a live region, because the focus goes back to "Change…", which
says nothing of it. A removal that could not take every one says why in `danger`
- "Not every old backup could be removed: …" when some went, "The old backups
could not be removed: …" when none did - and leaves the question up, about how
many are left rather than how many there were; Keep then takes the sentence away
with the question. A change that left no snapshot to ask about says only "Master
password changed." No notice rises for any of it: the row is where it is said.

**A lock after a change says so on the unlock screen.** The row that said which
password opens the vault goes with the window, and a change pressed as the lid
closed may never have said it at all. So the first unlock screen after a lock
that closed a vault whose password changed while it was open says "The master
password was changed before locking, and the vault opens with the new one." in
the place and the type of the line about typing a lock saved - `small`,
centred, `mt-6`, in `txt2`. It is gone once the vault is open again, and is not
said about any other file. The question about old backups is not asked again.

## The menu bar and the keyboard

The mockup draws no menu bar and no list of keys. Both follow from the window:
the bar is the system's, with Coffer's items named the way a Mac names them,
and the list is drawn from tokens the window already uses.

**Keyboard Shortcuts is a sheet over the window, under the title bar.** It is
the conflict dialog's veil and card with two lists in it: `canvas/75` fading in,
the `raised` card with its hairline and `rounded-md` rising, at the mockup's
`max-w-[720px]` so the lists stand side by side, and scrolling inside itself
when the window is at its smallest. The title is `text-title`; each list is
headed in the mono label the window puts over a column (`text-label`,
`tracking-label`, `txt4`, upper case), and each row is what the key does in
`text-small` `txt2` with the key in the search field's own key cap at the end,
`line` between the rows. The first list is the keys of Coffer's items in the
menu bar, the second the keys the window answers itself and the presses that
choose rows in the list, with the system's Close Window, which locks the vault
here. The one button is the plain bordered
pill, "Close", and it is where the focus lands; Escape, Close, a press on the
veil and anything chosen in the menu bar put the sheet away. Put away from
itself, it gives the focus back where it was, once the screens under it are no
longer inert; put away by a choice in the menu bar, it leaves the focus to the
choice. The title bar stays above the veil, so the window can still be dragged
and locked while it is open, and every key is the sheet's while it is up,
wherever the focus is - on the title bar's Lock button one Shift+Tab away, or
nowhere after a press on the title bar - so Cmd+C copies nothing out of the pane
under it and Escape puts away the sheet and not the pane. `Sheet.svelte` is the
veil and the card, for any sheet like it.

**The Lock button names its key**, "Lock the vault · ⌘L", as a tooltip in the
form the list row's copy buttons already use.

**Cmd+C keeps the mockup's promise, and the menu's Copy Password is
Shift+Cmd+C.** Edit ▸ Copy is the system's, and every text field finds Cmd+C
through it, so no item of Coffer's can take the key. Cmd+C with nothing selected
still copies the open entry's password, because the page answers it before
AppKit looks in the menu.

**Move to Recycle Bin is grey while a field is being written.** In a field
Cmd+Backspace deletes to the start of the line, as it does everywhere on a Mac;
Finder greys its own Move to Trash while a name is being edited, for the same
reason. It is grey as well for an entry a deletion would erase, because the item
says the bin: that one is deleted from the pane, which asks first. While rows
are chosen it acts on them rather than on the open entry, and only when every
one of them goes to the bin; a choice holding one that would go for good is
deleted from its bar, which asks.

**Duplicate is offered while a field is being written.** Cmd+D means nothing to
a text field on a Mac, so the item stays when the focus is in one; chosen there,
the field is left first, which writes it, and the copy holds what was typed - a
tag or a field's name being typed included. It is grey with no entry read, for
an entry in the bin and in a vault Coffer will not write, where the pane draws
no Duplicate either. It is grey while rows are chosen too: Move to Recycle Bin
then acts on the choice, and a copy of the pane's entry - which need not be one
of the rows - would be one entry nobody chose, made over a choice it let go of.
The pane's own pill names its entry, and stays.

## Menus under the pointer

The mockup draws no menu under the pointer. A native one is AppKit's and wears
the system's look, so nothing in it is a token; what is Coffer's is which menus
there are, what each line says, and what the window does around them.

**A right-click is Coffer's, and WebKit's menu is left only to a field being
typed in.** A row in either list, a folder in the tree or in the bin, the bin's
own row, a field of the reader's own, the password's row, a file and a revealed
value each draw Coffer's menu where the pointer is. The name of the password's
row and of a field of the reader's own belong to that row, and draw its menu.
Anywhere else - "All entries", "Not in a folder", blank space, the name over a
login, an address, notes or tags - draws none. In a field the reader types
into - the search, a value being written, a name, a new password, the three of
the master password in the settings - WebKit's own menu stays, because its Cut,
Copy, Paste and spelling are about their typing; a field made read only, as
those three are while a change is on its way, draws none. A revealed value is
not typing: its menu is Copy, through Rust, and Hide, and Look Up, Translate,
Search and Share never appear on it. Nor do they anywhere else while part of a
value is still selected: WebKit's menu is about the selection, wherever the
pointer is, so a right-click in the search field then draws no menu at all.

**What the menu is about is marked while it is open.** The row, the folder, the
bin's row or the file carries the accent hairline a field takes with the focus,
drawn inside its edge so that nothing moves (`[data-menu]` in `app.css`: one
pixel of `accent`, offset inward), so that the reader can tell which of thirty
rows the menu is for. It goes when the menu closes. A right-click on a chosen
row marks every chosen row, and the menu is about all of them; one on a row
outside the choice is about that row alone, and the choice stays. A field and
a revealed value are not marked: the pointer is on them, and a value's
selection is drawn already.

**Every item does what its button does, and asks where its button asks.**
Delete Forever… on a row opens the entry with its question already put, the
focus on "Keep it", in the bin card or at the foot of the pane; on several
chosen rows it puts the bar's question. The bin card's question goes with the
card: an entry or a folder that leaves the bin while it is up - Put Back from
another menu, a reload - is asked about nowhere, and is not found asked if it
comes back. Move to Recycle Bin… on a folder shows
the folder and puts the folders pane's question; Remove… on a file puts the
file row's; Empty Recycle Bin… shows the bin and puts the question under it.
New Entry Here and New Folder Inside show the folder and make there, the way
"+ Entry" and the folders pane's plus do. Rename shows the folder and puts the
focus in its name with the name selected - and the folders pane's own Rename
now does the same, where it used to open the field and leave the focus behind.
An item that would take the pane away while it holds a new password does
nothing, and that field's question rises, as with its button. An item that
does not apply is drawn greyed out rather than left out: Copy Password on an
entry with none, Open Address for an address Coffer would not open, every
change in a vault Coffer does not write back, whichever reason the note behind
Read only gives, and Save to… on a file of a KDBX 3 vault, which Coffer does
not read out.

**Two parts of what a hidden field could offer are left out.** The item is
Copy, not "Copy (clears in 1 min)": the notice every copy puts up says how long
the clipboard holds it ("Password copied. The clipboard clears in 1 minute."),
from the number Rust keeps in the settings and the words `duration.ts` writes
it in, and a title saying the same would be a second place for that number
and a second way of writing it, which goes stale when the reader changes the
setting. And a field menu - Show, Copy, Change, Make a New One, Remove - is
drawn for the password's row and for the reader's own fields only. A login, an
address, notes or a title that a database written elsewhere protects keeps its
eye and its copy beside it, and its value, once shown, draws the value's menu;
a right-click on its mask draws nothing. Its Change is a line of its own whose
state the entry pane keeps, which a menu inside the row would have to reach
across. Both could be built - Rust holds the clipboard's timer, and could put
it in the title - and neither is, for those reasons.

**The words are the menu bar's where the item is the menu bar's.** Copy Login,
Copy Password, Duplicate and Move to Recycle Bin are called what the bar calls
them; an item that asks first ends in an ellipsis, as the bar's do. No key is
written beside an item: the bar's keys act on the entry in the pane or on the
choice, a menu's items on what was right-clicked, and the same key beside both
would say they were one.

**Move to lists the vault's folders as the folder list does.** "Top of the
vault" first, then the folders in the tree's order; a folder with folders inside
it is a submenu headed by itself. Where the thing is already is drawn greyed
out, a folder's own line among them, and nothing under a folder being moved is
listed. The recycle bin and everything in it are left out: deleting is the way
in. A name is written whole up to sixty characters, then an ellipsis, and set
apart from the menu's own words the way a name is set apart in a sentence.

## Motion

The mockup is a still picture and names no duration. The window has three, and
nothing defines a fourth:

- **a rise**, 170ms, for something that came from below or grew out of a press: a
  notice, a generator, a list of versions, a name being typed;
- **a fade**, 130ms, for something already the size it is that only had to
  appear: a pane, a dialog's veil, an empty state;
- **a pop**, 120ms, for a list that belongs to the control under it, growing out
  of that control's edge. The dropdown on the settings screen, the folder list,
  the list of kinds under "+ Entry" and the note behind Read only are the four.

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

## Choosing several entries

The mockup draws one entry at a time. Choosing several, and acting on them
together, is built from its tokens.

**A chosen row is drawn on the selection colour.** `selection` is the mockup's
colour for selected text, and a chosen row is a selection. The wide list's band
and the narrow list's card take it as their plane - the card with its hairline
`transparent` - with the name in `txt`, every other line in `txt2`, and the key
or clip in `txt3`: `txt3` and `txt4` as text fall under 4.5 : 1 on it, and
`theme.test.ts` holds `txt` and `txt2` to 4.5 there in both themes. A tag chip
keeps its own `surface2`. The entry open beside the narrow list keeps its accent
bar and its hairline over the plane. A row is drawn chosen only while the bar is
drawn, so the entry a plain press opens looks as it always did, and a screen
reader hears ", selected" after the title of each chosen row. How many are
chosen is said as it changes, from a region that is never drawn and stays while
the bar comes and goes: Cmd+A and a Cmd-click leave the focus where it was, on
nothing that would say so.

**Cmd, Shift and Cmd+A.** Cmd-click chooses a row or lets it go and opens
nothing; Shift-click chooses every row from the last one pressed to this one, in
the order the list draws them, on top of what Cmd chose before, and a second
Shift-click moves the end of that run rather than adding another. A plain press
opens the row and makes it the whole choice; Control-click is the menu under the
pointer and chooses nothing. Cmd+A chooses every row the list draws - after a
search, only what it found, and in the bin never the deleted folders above the
entries - unless the focus is in a field or in the entry pane, where Select All
is the system's. A vault Coffer will not write back offers no choosing: every
press there opens.

**The bar stands where the list's column names were,** in their own type -
`label` mono, upper case, the label tracking, `py-2` and `px-5` over a `line` -
so nothing under it moves: "3 selected" in `txt2`, then "Move to…", "Add tag" and
"Delete" in `txt3` that turn `txt` under the pointer, Delete turning `danger`,
with a `·` in `txt4` between them, and at the end the cross at 14 pixels in
`txt4`, "Deselect all · Esc", which lets go the way Escape does. In the bin it
offers "Put back" in the accent and "Delete forever…". Beside an open entry the
list has no column names, and the bar is a line of its own above the cards that
stays put while they scroll. When anything chosen would go out of the file for
good, Delete reads "Delete forever…" and asks first in the window's question box
under the bar, on `surface2` with the bar's own margin - "Delete 3 entries
forever? This can’t be undone.", "Keep them" first and "Delete forever" last in
red. "Add tag" gives way to the tag chip's own field, drawn into the bar's
padding (`-my-1`) so the bar keeps its height; Return puts the tag on, and
Escape or leaving the field lets it go. Unlike an entry's own chip, leaving does
not put it on: here a tag is a version on every chosen entry, and the press that
took the focus - the bar's own Delete, anywhere else - was not the reader saying
it was done. The window going behind another app is not leaving, and the field
keeps what was typed. "Move to…" opens the folder list an entry's line opens,
on the pop, under the bar at its padding and the notice's 292 pixels, with the
folder every chosen entry is in marked, when they are all in one. Escape in any
of the three closes it and goes no further. Choosing anything else closes
whichever is open, because it was about other rows.

**What is chosen is what is drawn.** A search that hides a chosen row lets go of
it, and clearing the search does not bring it back; another folder lets go of
all of them, and so does the file read again; a change that takes a row out of
the list lets go of that row. Nothing the bar does reaches an entry the reader
cannot see. Escape lets go of the choice before it puts the pane away: the
search first, then the choice, then the pane.

**One notice for the lot,** the notice the window already draws for a change it
can take back, its Undo taking back the whole batch: "Moved 3 entries to the
Recycle Bin" with the trash icon, "Moved 3 entries to “Banking”" and "Put back 3
entries" with the folder icon, "Added “work” to 3 entries" with the check, each
icon in `txt3`. One entry is named by its title, isolated, as everywhere. A
batch nothing can take back says so without the button - "Deleted 3 entries
forever" - and so does a tag every chosen entry already had, "3 entries already
have “work”", and a put back in a vault that keeps no bin, where the undo could
only be refused. An undo the vault has moved on from - this one or any other in
the window, a removed field's and a single move's included - says "Something has
changed since, so that can no longer be undone." and does nothing.

**Dragging a chosen row carries the choice.** A drag that starts on a chosen
row carries every chosen row, and the notice names them all; one that starts on
a row not chosen carries that row alone and leaves the choice as it was.

## One size that was off the scale

The tag's remove cross was 12 pixels where the mockup's smallest icon is 14.
