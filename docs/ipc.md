# The window and the vault

`vault-gui` is the only part of Coffer that knows there is a window. It holds
one open database, turns what `vault-core` hands back into messages the webview
can read, and refuses to send anything else.

Everything below is the contract between the two halves: what crosses, what
deliberately does not, and the traps in Tauri that shape it.

---

## The rule the whole boundary exists for

The frontend gets metadata. A value the database protects is not in any message
except the one answer to an explicit reveal, and the master password only ever
travels the other way, as bytes.

| Action | What reaches the webview |
|---|---|
| Tree and list | id, title, username, URL, tags, dates, whether there is a password, how many attachments, what deleting it would do, and for anything in the recycle bin when it went in and the folder it came from |
| Open an entry | the same, plus every field's name, kind, whether it is empty and whether it is in lines, plus attachment names and sizes, plus what deleting it would do |
| Reveal a field | the value, one field, once |
| Read a previous version | the same as an entry, and one value at a time on a reveal |
| Copy a field, or the part of it selected on the screen | nothing; Rust writes the pasteboard |
| Open an address | nothing; Rust hands the URL to the system |
| Add a file | nothing; Rust opens the panel and reads the file |
| Write a file out | nothing; Rust opens the panel and writes it |
| Make a password | the password, once, the way a reveal answers, and which of the kinds of character asked for it happens to lack |
| Hide a field, or rename it | nothing new; the value moves inside Rust, and a hidden field comes back with no value |
| Show a field | its value, in the entry that comes back: it is an open field from then on, and crosses as every open value does |

A field's `value` is `null` exactly when it does not cross: the database
protects it, or it is the password. **A password never crosses, protected or
not.** A file can hold a `Password` field the database left unprotected, and it
is still a password; `empty` says whether there is one to ask for.

`lines` is one more bit of the same kind, sent for every field: whether the
value has a line break in it. It says nothing of what the value is, and it is
what opens the Change of a protected value in lines - ten recovery codes - in a
field written in lines, where Return starts the next code rather than saving
the first over all ten. Rust reads it off the value in
[`Field::in_lines`](../crates/vault-core/src/model.rs).

## The commands

| Command | Takes | Answers |
|---|---|---|
| `status` | | the chosen database, whether it is open, how many entries, whether it can be written, why it locked, whether that lock saved something being typed and whether some of it was kept beside the value it was for, the unsaved copy sitting beside it, the chosen file as it stands on disk, the vault it was copied from when it is a lock's copy, and - when nothing is remembered - a vault found in Coffer's own folder |
| `choose_database` | | the database the user picked, or nothing if they closed the dialog |
| `choose_found` | | the vault the last `status` found in Coffer's own folder, now chosen |
| `unlock` | the master password, as the raw body | nothing |
| `lock` | | nothing |
| `tree` | | the root group, its sections and their entry rows |
| `entry` | `id` | one entry's fields, attachments, tags, dates and version count |
| `reveal` | `entry`, `field` | the value of that field |
| `copy` | `entry`, `field`, `range` | the seconds until Coffer clears the pasteboard |
| `open_url` | `entry` | nothing |
| `snapshots` | | the `.bak` files beside the chosen database, newest first |
| `choose_snapshot` | `index` | the snapshot now chosen |
| `choose_rescue` | | the unsaved copy now chosen |
| `discard_rescue` | | |
| `put_back_rescue` | | the vault, now at its own name again |
| `promote_rescue` | | the vault, which is what is open now |
| `leave_rescue` | | what is chosen afterwards: the vault the copy was taken from, or the copy when the lock that closed it kept its work elsewhere or lost it |

Everything slice 3 added:

| Command | Takes | Answers |
|---|---|---|
| `create_entry` | `group` | the tree, and the entry it made |
| `delete_entries` | `entries` (each an `entry` and the `deletion` the window showed), `sequence` | the tree |
| `create_group` | `parent`, `name` | the tree |
| `rename_group` | `group`, `name` | the tree |
| `delete_group` | `group`, `deletion` | the tree |
| `put_back_entries` | `entries` | the tree |
| `put_back_group` | `group` | the tree |
| `move_entries` | `entries`, `into` | the tree, and each entry that changed folder with the folder it left |
| `move_entries_back` | `moved`, `into` | the tree |
| `move_group` | `group`, `into` | the tree |
| `move_group_back` | `group`, `from`, `into` | the tree |
| `empty_recycle_bin` | | the tree |
| `set_field` | `entry`, `field`, `value`, `protect`, `sequence` | the entry |
| `remove_field` | `entry`, `field`, `forever` | the entry |
| `set_protection` | `entry`, `field`, `protect` | the entry |
| `rename_field` | `entry`, `from`, `to` | the entry |
| `undo_removal` | `entry`, `field` | the entry |
| `set_tags` | `entry`, `tags` | the entry |
| `add_attachment` | `entry` | the entry, or what already has the file's name, or nothing if the panel was closed |
| `keep_both_attachments` | `entry` | the entry |
| `replace_attachment` | `entry` | the entry |
| `withdraw_attachment` | `entry` | nothing |
| `export_attachment` | `entry`, `name` | nothing |
| `remove_attachment` | `entry`, `name` | the entry |
| `remove_attachment_and_versions` | `entry`, `name` | the entry |
| `versions` | `entry` | the previous versions, oldest first, and the revision they were listed at |
| `version` | `entry`, `index`, `revision` | one version, read like an entry |
| `reveal_version` | `entry`, `index`, `revision`, `field` | the value of that field in that version |
| `copy_version` | `entry`, `index`, `revision`, `field`, `range` | the seconds until Coffer clears the pasteboard |
| `restore_version` | `entry`, `index`, `revision` | the entry |
| `delete_version` | `entry`, `index`, `revision` | the versions that are left, and their revision |
| `clear_history` | `entry` | the versions that are left, which is none, and their revision |
| `generator` | `purpose` | the recipe that generator opens with, the lengths its slider runs between, whether that is a PIN's, and the characters "Symbols" and "Avoid look-alikes" stand for |
| `generate_password` | `recipe`, `purpose` | a password, the kinds asked for that it lacks, and the generator as that recipe settles it |
| `save` | | nothing |
| `save_over` | | nothing |
| `save_copy` | | the file it wrote, or nothing if the panel was closed |
| `reload` | `sequence` | the tree |
| `rival` | | when the file on disk was written and how many entries it holds |

**The generator's recipe and its limits are Rust's.** `generator` answers with
the last recipe that generator made a password from - kept in `generator.json`
beside the settings, owner-only, never in the vault - so a lock, which destroys
the window, does not cost the reader the generator they set up for their bank's
rules, and neither does a launch. `purpose` says which generator is asking: the
password's (`password`) or one under a field of the reader's own (`field`), and
each remembers its own, so a PIN made for a card's field is not what the
password's generator opens with next. With the recipe come the lengths the
slider may take, four to sixty-four for digits alone (a PIN) and eight to
sixty-four for anything else, and the characters "Symbols" and "Avoid
look-alikes" stand for - none for a PIN, which keeps 0 and 1 because there is no
letter in it to take them for - so the screen holds no copy of any of them. `generate_password` settles the recipe it is sent
before it uses it - each kind once, a length inside the range, only characters
some kind holds kept in `avoid` - and answers with the generator as that settled
recipe stands, so a slider left on a PIN's four when a letter is added moves to
eight. The password is drawn evenly from what is left; Rust says which of the
kinds asked for it happens to lack, and the screen says so beside the button
that puts it in. It does not draw again until every kind turns up: that is a
rule of its own and a narrower set of passwords than the one the reader chose.

Everything slice 4 added:

| Command | Takes | Answers |
|---|---|---|
| `settings` | | the two timers, the two switches, the look, and the values each may be set to |
| `set_settings` | `settings` | what was actually stored, which is not always what was sent |
| `stirred` | | the seconds the open vault has left, or nothing when none is open |
| `draft` | `entry`, `field`, `value`, `protect`, `beside`, `sequence` | nothing |
| `default_new_database` | | where a first vault goes when nobody has said, and what is already there |
| `choose_new_database` | | where the reader wants the new vault instead, on the same terms |
| `target` | | the place the new vault would go, read again, on the same terms |
| `choose_existing` | | what already sits where the new vault would go, now chosen |
| `calibrate` | | how many Argon2id passes a one-second unlock costs here, and what that measured |
| `create_database` | the master password, as the raw body | nothing |

**A creation is an unlock that writes the file first.** Where the vault goes and
what its key derivation costs are settled by the commands before it, for the same
reason the database to open is: the password is the whole body of the message and
nothing can travel beside it. `create_database` refuses a JSON body exactly as
`unlock` does.

**The place is settled before the screen is drawn, not by a panel.**
`default_new_database` picks `~/Coffer/vault.kdbx` and keeps it, so that making a
first vault is a password and nothing else - which is what the slice's five
actions are counted against. It writes nothing; the folder is made at the moment
the reader commits, where a refusal can still be reported. `choose_new_database`
is the same answer through a save panel, for a reader who wants it somewhere
else, and it is the only one of the two that a locked vault guards, because the
picker is reachable from a screen a vault can be open behind. The folder and the
name are written down once, in [`home.rs`](../crates/vault-gui/src/home.rs),
which is also where a launch that remembers nothing looks.

Both answer with a `Target`, which carries only what the creation screen draws:
the path as the reader would write it (`shown`, starting with `~` under the home
folder, for the screen and never for opening), and `standing`, what is already
at the name. `free` is nothing; `vault` is a file with something in it, a link
to one included; `copy` is nothing at the name but the copy a lock left of a
vault by that name beside it; `empty` is an empty file, which is what a creation
killed half way leaves; `other` is a folder, or a link to one or to nothing. The
rule is the one the search of `~/Coffer` uses, in
[`home.rs`](../crates/vault-gui/src/home.rs). Nothing is ever made over anything -
a creation takes no snapshot - so the screen says so before a password is typed.
For `vault` and `copy` it offers `choose_existing`, which points the session at
the name: a vault opens there, and a copy is put back from that name's unlock
screen. `empty` and `other` are named for what they are and the way on is
"Somewhere else"; offering to open them would be an unlock that answers "not a
database" or "gone". A save panel's own "Replace" does not change any of it.

`standing` is advice read off the disk a moment earlier. `choose_existing` reads
the name again by the same rule and answers `gone` for anything but a vault or a
copy, and the screen then asks `target` for the place as it stands now, without
the place changing. The creation itself takes the name with an exclusive create,
refuses a name with a copy beside it, and either refusal is `taken`, after which
the screen reads `target` as well. While the place is not free, the screen also
reads `target` each time the window gets the focus back, since the way on for
`empty` and `other` is the Finder and nothing on the screen would read it.

**A vault is written down when it opens, not when it is picked.** Every vault
that opens - by an unlock or by a creation - goes through one function in
`session.rs`, which is the only place a vault is put into the session and the
only place the path is written for the next launch; a test reads the source to
keep it that way. Coffer 0.1.0 wrote the path down in `choose_database`, so a
vault made in Coffer was never written down and the next launch showed its owner
the first-run screen. A snapshot and the copy a lock left are never written
down, however they came to be open: they open with the vault's password and are
older than it. A write that fails costs the reader a pick next launch and does
not fail the unlock.

**A launch that remembers nothing looks in `~/Coffer`.** When `status` has no
database it also answers `found`: the file name of a vault in Coffer's own
folder, and the folder's name, for the sentence the first-run screen says above
the offer to make one. The path does not cross; the session keeps the one it
named, and `choose_found` opens that file and no other. It asks the file again
by the search's own rule and answers `gone` when it is no longer a vault, rather
than searching again and opening whatever else the folder holds under a card
that named another file. The screen then reads `status` again, so the card says
what is there now or goes. What counts is a file whose name ends in `.kdbx` -
not a snapshot, not a copy a lock left, not a folder, not an empty file, not a
hidden one - and a link counts when it leads to one. A copy a lock left whose
vault's file has gone counts under the vault's name and by the copy's time: it
is all there is of that vault, and the unlock screen for the name puts it back.
`found` then says `copy`, and the card says it found a copy of the vault and
that the vault's own file is not there, rather than that it found a file the
Finder will not show.
With several, `vault.kdbx` wins, then the one written last, then the first by
name. A folder that cannot be read is one in which nothing was found.

**A launch that remembers another vault opens its panel in `~/Coffer`.** Coffer
0.1.0 wrote a vault down when it was picked and never when it was made, so a
reader who once picked an older file and then made a vault in Coffer launches
onto the older file, whose password is not the one they use. `SPEC.md` gives the
unlock screen the path, the password field, the button and two small links, and
says "Nothing else", so no second offer is drawn on it. Instead, when
`~/Coffer` holds a vault that is not the remembered one, "Open another database"
starts there, which puts the vault they use one press away from the screen they
are on. Otherwise the panel starts beside the remembered vault.

**`settings` sends the lists as well as the values.** What a reader may choose is
Rust's to decide, and a screen holding its own copy would be a second place the
answer lives. What comes back from `set_settings` is what was stored, because a
value the screen does not offer is settled onto one it does.

**The look crosses as a word, and each side does the half only it can.**
`settings` sends `theme` as one of `system`, `dark` or `light`, and
`themeChoices` as the three of them, the same way it sends the two lists of
seconds. Rust builds the window with it, which is what decides the traffic
lights, the native file panel and what `prefers-color-scheme` reports inside the
webview, and it hands the same word to the application when the reader changes
one — so a new look costs nothing and does not take the open vault with it. The
window writes `data-theme` on the `html` element, resolving `system` through
`prefers-color-scheme` itself, so `app.css` holds one light palette rather than a
third copy for a look that is really a question. Dark is the default, which is
what every settings file written before slice 5 reads as. A word this version
does not know settles on dark rather than being refused: a later Coffer offering
a fourth look would otherwise take both timers down with its own name.

**`stirred` is deliberately rare, and it can lock.** The window sends it on real
input and at most once every fifteen seconds. A key counts on its way in, before
anything on the page answers it, so one a sheet over the window keeps to itself
still says the reader is there. A status bar that asked once a
second would be an idle timer resetting itself, and a vault that never locks. A
stir that arrives after the time has already run out locks the vault rather than
starting the clock again: the timer's own wait is counted on the clock that
stops while the Mac sleeps, so after a night shut the reader's first key gets
there before it does, and a stir that restarted the clock would hand a fresh
timeout to whoever sat down. The lock runs on the thread that posted the stir, so
`stirred` is answered off the thread the window is drawn on, like every command
that can reach the lock.

**A command that changes something answers with what it changed.** A change to
one entry answers with that entry; a change to the shape of the vault answers
with the whole tree. The screen never patches its own copy of the database from
what it thinks a command did, because a screen that guessed wrong would go on
drawing something that is not in the file.

**What is typed reaches Rust before the field is left.** A value is written by
`set_field` when its field is left, and until then it was in the window and
nowhere else. A lock destroys the window, and the triggers that lock on their
own - sleep, the screen locking, Coffer quitting - arrive on the thread the
window is drawn on, where neither a message into the page nor its answer can get
through: a lock that asked the page to finish first could only wait for nothing.
So the page never is asked. It sends `draft` for a field a quarter of a second
after the last key, at least once a second while the keys keep coming - a steady
typist never pauses that long, and a lock then would cost everything since they
last stopped - and at once when the window loses focus, with what is in the
field; `value` is `null` when the typing was taken back - Escape, Cancel,
Discard, a field left as it was, a Change field emptied again, or a new password
whose entry the pane stopped showing. Rust keeps the last of it per field,
beside the open vault, in `Zeroizing` storage whose `Debug` prints `[redacted]`,
and every lock writes it into the vault before it wipes it (see Locking). The
Lock button waits for every draft and every value already on its way before it
asks for the lock. The text travels the way `set_field`'s does, window to Rust,
and nothing comes back.

**Only values are drafted, not names.** A lock writes what was typed into the
value of any of an entry's fields, the standard five and the reader's own, in
the box where the value stands and in a Change field. It does not write a tag still
being typed, the name in the step that adds a field, the name of a new folder
or a folder being renamed: those go to Rust when they are finished, with Return
or by leaving the box, and a lock before that loses them. That is a decision,
not an omission. A draft finishes a value the vault already has a place for,
and a lock is not the reader saying a name is done: half a tag would be a label
on the entry and in the vault's list of tags, half a field name a field called
something nobody chose, half a folder name a folder made or renamed - each a
change to the shape of the vault rather than text left in a box, and each one
the reader would have to find and undo. What is lost is a word or two the reader
typed a moment before and can see is missing, never anything the vault held.
The search box, the generator's options and the unlock and creation screens
hold nothing of the vault and are not drafted either. None of these boxes
carries the dot that marks typing not yet written.

Every word about typing carries `sequence`, from one count per window that only
goes up and starts at the moment the window was built, in microseconds, so that
a word from a window a lock destroyed is older than any the next one sends.
Tauri runs commands side by side, so a draft sent before its field was written
can arrive after it. Rust drops a draft that is no newer than the last word it
heard about the field - a draft, a draft taken back, or the value `set_field`
wrote, which carries its own number and ends the draft under the same lock as the
write - or than a `delete_entries` naming its entry or a `reload`, which carry
one too and let go of everything typed into what they take away. A restore is
not on that list, because nothing on the screen goes with it: every value typed
into a field of the entry was written when the field was left, before the
Restore button could be pressed, and a new password still waiting in its own
field with the question under it is still the reader's.

A draft is written only into a field the entry still has, or one of the five
standard ones every entry is drawn with, and only when it differs from what the
field holds; a draft the vault refuses is let go and the lock goes on. When the
lock's save went through, `status` answers `typed`, and the unlock screen says
"What you were typing was saved before locking." - a flag and nothing more,
because after a lock nothing of the vault is left in memory to name the entry
with. `typedBeside` is the second flag: some of it was a new value kept beside
the value it was typed for (below), and the screen adds that it was kept in a
field of its own and that the old value is unchanged. "Saved" on its own reads
as the new password being the entry's now. When the save went beside the vault
instead, the typing is in that copy with everything else, and `rescue` is what
is said.

**A new value typed in a Change field is kept beside the value, never over it.**
`beside` is true for text typed in a Change field of its own rather than into
the field where it stands. The reader has not saved it: it may be the first
half of a new password, or the wrong one pasted while the right one is fetched,
and a lock that wrote it over the stored password handed them one that opens
nothing, with the real one pushed into Versions. So a lock writes it into the
field only when the field holds nothing, and otherwise into a new protected
field of the same entry named after the one it was typed for -
`Password (typed before locking)`, numbered past a name the entry already uses
by the rule that names a second file - and leaves the stored value as it was.
That field is a standard KDBX string field of the kind the reader makes with
"+", which KeePassXC and every other client show and edit like any other; it is
not a field of Coffer's own, and nothing is added to the format (`SPEC.md`,
section 12). The unlock screen says what it always says and names nothing. The
rule is `Vault::set_typed`'s, in `vault-core`, and not the window's.

**A version is addressed by `(entry, index)`.** The index is its position in the
entry's history, which is the only thing that identifies one: modification times
have one-second resolution, so two versions written in the same second are
indistinguishable by date, and the list is ordered by date rather than by
position because a file another client wrote may hold them in any order.

**A save moves those positions,** because it brings every entry's history inside
the database's limits, so a list read before a save names versions that are no
longer there. The window reads the list back after the save rather than before
it, which is why every change is `save` and then `versions` and never the other
way round. Every write is followed by that reading, whichever entry it was to.

**A position goes back with the revision it was read at.** Rust answers one
command at a time and a save holds it for a key derivation, so a press made
while a save is running reaches the vault after the save has moved the
positions. A drop pressed in that second dropped the neighbour of the version
on its row, for good; a second Restore put another version over the one chosen.
So `versions`, `delete_version` and `clear_history` answer with
a `revision`, a number the session moves every time the open vault changes -
every edit, drop, restore, save, reload and move, counted by the vault itself
in `Vault::edits`, and a vault that opens - and not for a draft, anything that
only reads, or a change the vault refused or found nothing to do in. The window
reads no list again after a refusal, and a revision moved by one would have
turned the next press on a version into a refusal of its own. Every
command that takes an `index` takes that `revision` as well, and refuses with
`versionsChanged` and does nothing when the vault has moved on, checked under
the same lock the action runs in (`Session::at` and `Session::at_mut`). The
number is a counter and carries nothing of what the vault holds. `model.ts`
calls the pair a `Position`.

The window keeps presses away from a list it knows is out of date. When an
edit to the entry in the pane lands, its list goes - no rows, no count, and any
open question or version view with it - until the list read after the save
arrives. Commands run side by side, so a list asked for before that edit can
be read after it; the window counts every time its list went out of date and
drops an answer asked for before the count last moved. While a restore, a drop
or a clear is on its way, every further press on the versions is let go. A
`versionsChanged` that gets through anyway is answered by reading the list
again and saying so in a sentence: "The versions changed while you were
choosing, so nothing was done."

**A position is an answer about one entry.** The commands that list versions
answer with the list alone, so `ipc.ts` hands each list on paired with the entry
it asked about (`History` in `model.ts`), and the window draws a list only under
that entry. Rust answers one command at a time and a save holds it for a whole
key derivation, so an entry chosen right after an edit is read after the save:
the pane draws the row at once, and an entry, a list of versions or a failure
that arrives for an entry the pane has since left is dropped. An edit answered
that late is still saved and the tree still redrawn, because the change is in
the vault either way; only the pane is left alone. The same goes for a move to
the bin answered after another entry was opened: it is still offered back, and
its undo puts the entry back without opening it over the one being read.

**A removed field is taken back by Rust, in one call.** Removing a field
writes a version, and restoring that version is the undo - but only while the
removal is the last thing that happened to the entry. Any change since writes a
newer version, the save after the removal may prune it, and a version another
client dated later stands in front of it; restoring whatever is newest then
would take back more than the field. So `undo_removal` takes the entry and the
field's name and nothing else, and Rust decides which version puts the field
back and restores it under one lock, so nothing can land between the question
and the restore. When the removal is no longer the last thing that happened, it
refuses with `superseded`, restores nothing, and the window says the removal
can no longer be undone.

**A removal nothing could take back is asked about first.** A database whose
`HistoryMaxItems` is 0, or whose `HistoryMaxSize` the removal's version does not
fit, drops that version at the very next save. `remove_field` works that out
with the rule the save prunes by (`history::keep` in `vault-core`), and refuses
with `forGood`, doing nothing, unless `forever` is true. The window sends
`false` first; `forGood` opens a question in the field's row, and its red answer
sends the removal again with `true`. The question and the removal are one call,
so what the reader was asked about is what the vault held when the answer
arrived. After such a removal the notice says the field went forever and offers
nothing.

The offer lasts eight seconds, on the notice's own button and on Cmd+Z when the
key is not aimed at a text field, and it runs once however it is asked for. The
button does not take the focus when it is pressed, so a field holding typing is
not left, and does not write that typing, on the way to it. The offer is
withdrawn by the notice going and by a newer notice, and a lock takes it down
with the window. Nothing else withdraws it - not another entry opened, the pane
put away, or another change written: both kinds of undo act by id and never open
anything over the reader's later choice, and one the vault has moved on from is
refused by Rust at the press. A removal whose save failed is not offered back at
all: the failure has a notice of its own.

**What a deletion will do is said before it happens.** Every entry and every
group carries `deletion`, `bin` or `forever`: whether deleting it moves it to the
recycle bin, where it can be put back from, or takes it out of the file. It is
`forever` for anything already in the bin, for everything in a vault whose
`RecycleBinEnabled` is false, and for a folder the bin sits inside, which cannot
go into itself. The rule is one function in
[`vault-core`'s `bin.rs`](../crates/vault-core/src/bin.rs), and it is the same
function the deletion asks, so the answer the window drew is the thing that
happens. The window asks before a deletion that is `forever` and offers the
other kind back from its notice.

`delete_entries` and `delete_group` take back the `deletion` the window showed -
for each entry, in the first - and Rust refuses with `deletionChanged`, deleting
nothing, when deleting the thing would now do the other. Two deletions can wait
on the session behind one save, and the thread that takes it first is not the
one that asked first: a
folder that goes into the bin ahead of an entry in it, or ahead of a folder in
it, makes the move the reader agreed to an erasure. Something put back out of
the bin before its erasure arrives is refused the same way. The window says
nothing was deleted and reads the tree and the pane again, which show what
deleting each now does.

It travels with the tree and with the entry rather than in `status`, on
purpose. `status` is read when the window is built; the tree and the entry are
read again after every change and after `reload`, and a file another client
rewrote may have stopped keeping a bin. A flag read at unlock would go on
promising the bin to a deletion that erases.

What the window says afterwards is read off the answer rather than off that
promise: `delete_entries` and `delete_group` answer with the tree, and the window
offers Undo only for something still in it.

**The bin says when and where from.** Anything inside the bin - an entry row,
an entry, a group - carries `binned`, `null` everywhere else and for the bin
itself. `since` is when it went in: its own `LocationChanged`, or that of the
folder it went in with. `within` is the id of that folder - the deleted one the
bin holds, however deep this sits inside it - and `null` for something deleted
on its own. `from` is where putting it back takes it, and only when that is
still somewhere to go: `null` when nothing was written down, when the folder
has gone, and when it is in the bin too. For something deleted on its own it is
the folder it was in, the format's `PreviousParentGroup`. For something that
went in with a folder it is where that folder came from, never its own
`PreviousParentGroup`: KeePass and KeePassXC write one on every move between
folders, and the deletion did not move it, so its own points at some older
folder it was not in when it was deleted. The window says "Deleted with
“Banking”" for such a thing rather than naming a folder it came from. Folders
cross as ids and the window names them from the tree, so a folder renamed since
is called what it is called now.

**`put_back_entries` and `put_back_group` take something out of the bin** and
answer with the tree. It goes back into `from`, or to the top of the vault when
`from` is `null`, and the window reads where it went off the tree. A folder goes
with everything in it. Both refuse with `refused` for anything not in the bin -
the bin itself, the top of the vault, an entry already put back - so an undo
that arrives after the thing came back moves nothing, and with `readOnly` on a
database Coffer will not write back. Putting back is a move and not an edit: no
version is written, the modification time stays, and nothing is added to
`DeletedObjects`; the same is true of the move into the bin.

**`move_entries` and `move_group` move things between folders, and a batch
moves whole.** `move_entries` takes the ids of entries and the folder they go
into - the top of the vault is the root's id, which the tree already carries -
and `move_group` one folder and where it goes. Rust checks everything before
anything moves, so a batch holding one entry it refuses moves none: anything in
the recycle bin, the bin itself included, is refused with `refused`, because
putting back is the way out of it, and so is a destination in the bin, because
deleting is the way in and says first whether it can be undone. A folder cannot
go inside itself or anything under it, and the top of the vault goes nowhere.
Something already where it is sent stays, in its place in the folder, and a
move that finds nothing to do leaves nothing to save and the revision where it
was. A move is not an edit, exactly as a move to the bin is not: no version,
the modification time kept, nothing in `DeletedObjects`, and `LocationChanged`
and `PreviousParentGroup` written the way KeePass and KeePassXC write them. It
lets go of no typing and of no file waiting: an entry keeps its id wherever it
goes, so a lock writes what was being typed into it where it now stands.

**A move is taken back by what the file says of it.** `move_entries` answers
with the tree and `moved`: each entry that changed folder, in the order sent,
with `from`, the folder it left. That list is the undo. `move_entries_back`
takes it and the folder the entries went into, and moves each back to its
`from`, every one or none, only while the file still says the same: every entry
still in that folder, its `PreviousParentGroup` still the `from` it was answered
with, and neither folder gone or in the bin. Anything else is something done
after the move - another move of the reader's, or another client's read in by a
reload, an entry moved away and back by way of a third folder included - and
the undo is refused with `superseded`, moving nothing. The list is only a claim
the window makes: Rust checks it against the file and takes back nothing the
file does not bear out. Each entry goes to the end of the folder it came from,
because the library has no way to put one back where it stood. A folder's undo
is `move_group_back`, on the same terms: the folder, the folder it left
(`from`, which the window read off its tree before the move) and the one it
went into. Rust moves it back only while the file says the folder is in `into`
with `from` as its `PreviousParentGroup`, neither of them in the bin and `from`
not since moved inside it, and answers `superseded` otherwise. The window sends
neither undo while anything it is about - what moved, or a folder it comes out
of or goes back to - is still on its way somewhere: the undo and that move
would reach Rust in no fixed order.

Acting on several entries chosen in the list:

| Command | Takes | Answers |
|---|---|---|
| `tag_entries` | `entries`, `tag` | the tree, and the entries the tag went on |
| `untag_entries` | `entries`, `tag` | the tree |

`delete_entries`, `put_back_entries` and `move_entries` above are batches as
well; the entry the pane deletes or puts back is a batch of one, so there is one
deletion, one way out of the bin, and one rule for each.

**A batch is one call, one save and one undo.** Whatever the window does to
several chosen entries goes to Rust as one command naming all of them, is
written by one save and is offered back from one notice. Twelve commands would
be twelve saves - twelve key derivations, and twelve of the ten snapshots, so
the vault of an hour ago gone - and twelve notices, each withdrawing the undo of
the one before.

**A batch is all of them or none.** Every entry a batch names is checked before
any is changed, under the lock the change runs in: an entry that has gone, one
whose deletion would now do something other than the window showed, one
`put_back_entries` names that is not in the bin, a tag the format would split or
trim. One refusal is the whole batch's, nothing changes, and `revision` does not
move. An id that does not parse refuses the batch before the vault is asked.
Erasing files out of the pool, the one step that can still say no once the
checks are passed, is worked out for every entry together before anything is
written, and runs first (see `docs/vault-core.md`).

**A row says what deleting it would do.** `deletion` travels on every row of the
tree as it does on an entry and a folder, so the window knows before the press
which chosen entries go to the bin and which go for good, asks before the second
kind, and sends each back with the answer it showed. `sequence` lets go of every
draft of every entry named that was said before it, whether or not the deletion
goes through, and every file waiting on one of them goes under the same lock.

**A tag goes on where it is missing, and comes off only where it went on.**
`tag_entries` writes a version on each entry that lacked the tag and on no
other, and answers which those were; the undo sends exactly those to
`untag_entries`, so an entry that already had the tag keeps it. A tag every one
of them had already writes nothing, and the window says so and offers nothing.
`untag_entries` takes off every copy of the tag and holds it to no spelling
rule, since it is a tag the file holds; only an empty one is refused. A tag the
format would split, trim or drop is refused with `refused` and a sentence about
tags, which the window shows as it is, on the bar as on an entry's own chip.

**Putting back is a batch too.** `put_back_entries` is the undo of entries moved
to the bin, the bin's own Put back for several, and the pane's for the entry in
it, which offers nothing back: the pane stays on the entry and says where it
went. The bar's is offered back with `delete_entries`, each entry shown as going
to the bin, and only when the tree it answered says every one of them would: a
file whose bin was switched off after it filled still lists what is in it and
puts it back, but deletes nothing into a bin, and an undo that could only be
refused is no offer. An undo that would send the entry in the pane back to the
bin asks the pane first, as everything that takes an entry from it does, and
sends nothing while a new value typed into it waits there.

**An undo that came too late says so, in one sentence.** Every undo names the
refusals that mean the vault has moved on since: `superseded` for a removed
field or a move, `refused` for an entry no longer in the bin to put back,
`noSuchEntry` for one gone out of the file, `deletionChanged` for one that would
no longer go to the bin. Any of those did nothing, and the window says that
something has changed since, so that can no longer be undone, and reads the tree
again (`overtaken.ts`). Any other refusal is shown as Rust wrote it.

**`add_attachment` and `export_attachment` open their panel in Rust.** The
bytes of a file never cross in either direction and neither does a path: the
webview asks, the reader picks, and Rust reads or writes. The name a save panel
is offered comes from
[`Attachment::file_name`](../crates/vault-core/src/model.rs), which is the one
place a name out of a database is turned into a file name.

**A name the entry already gives a file is asked about, not written over.**
`add_attachment` answers `{ outcome: 'added', entry }` when the name is free and
`{ outcome: 'taken', clash }` when it is not, and in the second case nothing has
changed: no file has moved, no version has been written, and the vault has
nothing new to save. `clash` is the name as the entry holds it, `size` of the
file already there, `chosen`, the size of the one just picked, and `free`, the
name the new one goes under if both are kept. A taken name is an answer rather
than a failure because nothing failed: the window has a question to ask, and
a failure is a sentence to show.

The reader answers with one of three commands, each naming only the entry:
`keep_both_attachments` puts the file beside the one there under `free` (worked
out again when the answer arrives, so a name that came free or was taken in
between is honoured), `replace_attachment` takes the one there off the way a
removal would and puts the new one in its place, and `withdraw_attachment` lets
the file go. Replacing is refused with `attachmentInHistory` exactly as a
removal is, and the file goes on waiting, so the reader can still keep both.

The file waits in Rust, beside the vault, until then
([`offered.rs`](../crates/vault-gui/src/offered.rs)) - its bytes rather than its
path. A path is a question asked again later of a disk that has moved on: the
file can be renamed, rewritten by the scanner that made it, or on a stick that
was pulled out before the reader answered. The bytes are what they chose, they
cost what the pick had already paid to read them, and they are wiped the way the
vault is. The file is tied to the entry it was picked for: an answer naming any
other entry is refused and leaves it alone, and `withdraw_attachment` lets go of
it only for the entry it names, because the window sends that about the entry it
is leaving and the message can arrive after a file was picked for the next one.
It goes with the vault on a lock or when another database is chosen, when the
button is pressed again, when another file is picked for any entry, and when the
window stops showing the question. It also goes in Rust, under the lock of the
change itself, with a `reload` and with any change that leaves its entry
deleted or in the recycle bin: `delete_entries` naming it, `delete_group` on a
folder holding it, `empty_recycle_bin`. An answer already on its way, or queued
behind the change, is then refused rather than landing on a vault the reader was
never asked about, and nothing hangs on the window's `withdraw_attachment`
arriving.

A name that differs only in letter case is not taken. The format and every
KeePass client keep `Scan.pdf` and `scan.pdf` apart, so adding the second loses
nothing. The name Coffer makes up for keeping both avoids one that differs only
in case, because a Mac's disk would not let the reader write the two out side by
side. The number goes before the last extension - `archive.tar 2.gz`,
`id_ed25519 2`, `.env 2` - and after whatever the name already says rather than
counting on from a number in it, because `Tax return 2025.pdf` is a year. A name
held only by an earlier version is free on the entry: the version keeps its
bytes, and the question is about what the entry has now.

Nothing that names a file comes from the webview. `choose_database` opens the
system's own dialog and keeps the answer; `choose_snapshot` takes a slot number
and builds the path from the database the user already chose; `choose_found`
takes the vault the session is holding from the last `status`, and
`choose_existing` the place it is holding for a new vault. There is no command
that opens a path the frontend sends.

`open_url` reads the address out of the entry rather than accepting one, so the
only thing the webview can ask Coffer to open is an address it can already see -
and only when [`vault_core::url::openable`](../crates/vault-core/src/url.rs)
agrees, which is `http`, `https`, `mailto` and `ftp` and nothing else.

### Failures

Every command that can fail answers with `{ code, message }`. The message is
the sentence the screen shows, written in `vault-core` so that it never repeats
a secret and never says which half of a credential was wrong. The code is what
the screen branches on: `wrongCredentials`, `notADatabase`, `unsupportedFormat`,
`damaged`, `heldByAnother`, `externalChange`, `readOnly`, `gone`, `tooLarge`,
`noVault`, `noSuchEntry`, `refused`, `taken`, `attachmentInHistory`,
`versionsChanged`, `forGood`, `superseded`, `deletionChanged`, `needsOpening`,
`io`, `other`.

Where the right words depend on what the screen showed, the window writes the
sentence and the message is only a description of the refusal, kept out of
sight: `needsOpening` from `put_back_rescue` (the unlock screen's card), and
`externalChange` from `promote_rescue` (the banner over a copy). One sentence
lives in one place.

`versionsChanged` is a position read at a revision the vault has moved on from
(see above). Nothing was done, so it is not shown as a failure: the window
reads the list again and says the versions changed while the reader was
choosing.

`forGood` and `superseded` are about a removed field (see above), and
`superseded` about a move taken back as well; neither did anything. `forGood`
is answered with a question, and `superseded` with the sentence every undo that
came too late has (see above).

`deletionChanged` is a deletion that would no longer do what the window showed
(see above), and nothing was deleted.

`needsOpening` is `put_back_rescue` on a disk that keeps no second name for a
file, and nothing was moved: the unlock screen offers to open the copy instead
(see below).

`taken` is `create_database` finding a file where the new vault would go, or the
copy a lock left of a vault by that name beside it. The creation screen reads
the place again with `target`, says what is there in its own sentence, which
names the place and the way on, and hands back the two passwords it emptied on
submit: this is the one refusal that says nothing about them. It is also
`put_back_rescue` finding something at the vault's name by the time the copy
would go there, and the unlock screen says so in a sentence about the move (see
below).

`attachmentInHistory` is the one the entry screen has an answer for. Removing a
file is refused while previous versions of the entry still hold it - the format
keeps them inside the entry and nothing can rewrite one - so the screen offers
to clear those versions and remove the file, which is the only sequence that
works and is the reader's to choose. That offer is `remove_attachment_and_versions`
rather than the two commands one after the other: a removal can be refused for
more than one reason, and the versions go back if the file still cannot go.
`replace_attachment` answers with the same code for the same reason, and the
entry screen asks its question again without the answer that was refused.

`externalChange` is the one the conflict dialog is built on. `save` answers with
it when the file is not the one the vault was opened from, and nothing has been
written at that point. The screen then has three ways out and each of them keeps
something: `reload` takes the version on disk and drops what is in the window,
`save_copy` writes what is in the window to a file of its own first, and
`save_over` writes over the file - with the version that was there going into
`<database>.1.bak` on the way, so it can still be opened afterwards.

A rejected command rejects with that object and not with an `Error`, so
`instanceof` and `.message` are both useless on the raw value. `asFailure` in
`src/lib/ipc.ts` is the one place that reads it.

## The one value that goes the other way

A field the reader is editing crosses as ordinary JSON - its draft as well as
the value it is left with - and the master password does not. The difference is
not carelessness, it is what the two values are.

A password being changed is in the window already: it is the `value` of the
field the reader is typing the new one into, which is a JavaScript string by
construction, and no shape of message changes that. It is the reader's own,
typed by them; a value Coffer revealed is never in a field anything can type
into. The master password is different in one
way that matters - it is never displayed, never edited in place, and it opens
everything - so it is worth the machinery of a raw body, and `unlock` is the
only command that gets it.

What the boundary is for still holds in both directions: a value the database
protects **leaves** the vault only through `reveal`, `reveal_version` or
`generate_password`, one value at a time and only when the screen asked.
`set_field` carries `protect`, which says how a field the entry does not have
yet is made. A field it has keeps its own protection whatever `protect` says:
the screen sends what it read before the reader pressed anything, and a value
written on the way out of a field the reader had just hidden would otherwise
land after the hiding and put the value back into the file as plain text.
`set_protection` is the one way a field's protection changes, and it moves the
value between the two kinds of storage inside Rust, so a value the screen does
not hold is never asked of it. Showing a field is the reader choosing to stop
protecting it: from then on it is an open field, and its value is in every
entry the screen is sent, starting with the answer to that command. `rename_field` does the same for a name: the
value goes with it, protected as it was. Neither works on the five fields every
entry has, whose names are what KeePass clients know them by and whose
protection is the database's.

## A value selected on the screen

A revealed value is text a reader can select, because somebody reading ten
recovery codes off the screen needs to keep their place. What the system does
with a selection is not something Coffer lets happen to a secret: its copy is an
ordinary pasteboard write - no concealed type, no auto-clear, and the clipboard
history tools keep it - its menu under the pointer offers Look Up, Translate,
Search and Share, and a selection can be dragged into any other application.

So `guard.ts` takes the system's copy and cut wherever they land - on the
value's node or on chrome inside a selection that runs through the value, which
is where a selection begun in a label lands them - and hands them to Rust with
the part of the value that was selected, dropping the chrome around it. A
selection that reaches two values copies nothing at all, because Rust copies one
field at a time, and the window says so in a failure notice ("Select one value
at a time to copy it.") rather than leaving the reader to paste whatever the
pasteboard held before. `copy` and `copy_version` take a `range`, two
positions counted in UTF-16 code units the way the text node counts them, or
`null` for the whole value. `vault_core::SecretValue::part` cuts the value in
Rust and refuses a range the value does not have, an empty one, or one with an
end between the two halves of a character outside the basic plane. The
positions come from the selection's own offsets and never from its text, which
would be a copy of the secret in a JavaScript string. A right-click or a drag on anything inside such a
selection draws no WebKit menu and starts no drag. The generator's value is the
one revealed value with nothing in the vault to copy by name, and its copy is
refused with a sentence that says to put it in the field first.

`Cmd+C` follows the same rule. With the focus on the row of a protected value -
which is where Show puts it - it copies that row's value; with nothing selected
and the focus anywhere else, the entry's password, as the mockup has it. A
selection is left to the system's copy, which the guard sends to Rust when the
selection reaches a value. Text the reader is typing into a field is theirs, and
the system copies it as it copies anything typed.

What the page cannot see is what the system does without asking it. The
Services submenu is the one way in the menu bar: a service is handed the
selection by WebKit directly, with no copy event for the guard to take, and New
Sticky Note or a TextEdit window containing the selection keeps a revealed value
in plain text in another application. So Coffer builds its own menu bar
(`menu.rs`), Tauri's item for item less Services, with Coffer's own items among
them (see The menu bar). What is still open:

- A service's keyboard shortcut, such as Shift+Cmd+Y for a new sticky note, is
  the system's and not the menu's. Whether AppKit still runs one with no
  Services menu in the bar is on the pre-release checklist rather than assumed.
- Look Up by a force click, or Ctrl+Cmd+D, reads the word under the pointer
  without a menu or a copy.
- macOS adds items of its own to an application's Edit menu - Start Dictation,
  Emoji & Symbols and, where Apple Intelligence is on, Writing Tools - and
  Writing Tools works on a selection. Coffer cannot take them out from here.
- Anything the reader has granted Accessibility or Screen Recording can read
  the screen, a revealed value included, for as long as it is shown.

A version's value goes through `copy_version`, the copying twin of
`reveal_version`, because a version is read on the screen like the entry is
and a value selected there is as much a secret.

Rust cuts the value it holds now at the positions the screen measured, and
nothing tells it which text they were measured on. So a value on the screen goes
the moment its entry comes back changed - a new password saved, one made and put
in, a version restored, any change at all (`conceal` in `reveal.svelte.ts`,
from the window's one place a change lands). A reveal left up from before the
change was the old password under the label of the new one, and a part of it
selected and copied was cut out of the new value.

## The menu bar

Coffer's own items sit in the bar among AppKit's: Settings… (Cmd+,) and Lock
Vault (Cmd+L) in the application menu; New Entry (Cmd+N), New Folder
(Shift+Cmd+N) and Open Vault… (Cmd+O) in File; Find… (Cmd+F), Copy Login
(Cmd+B), Copy Password (Shift+Cmd+C) and Move to Recycle Bin (Cmd+Backspace) in
Edit, under the system's own; Keyboard Shortcuts in Help. `menu.rs` names each
and gives it its key, and the window knows each by the same word (`COMMANDS` in
`model.ts`) and draws the same key (`KEYS` in `shortcuts.ts`).
`shortcuts.test.ts` reads `menu.rs` and fails when the two disagree.

The commands the menu bar added:

| Command | Takes | Answers |
|---|---|---|
| `listen` | `channel` | nothing; what the reader chooses outside the page arrives through the channel |
| `menu_state` | `enabled` | nothing |
| `close_window` | | nothing |

**One route from the bar to the page.** Each item is a press the page answers
with the function its button runs - New Entry is the + Entry button's, Lock
Vault the Lock button's - so nothing on the screen is done two ways. Rust hears
the choice as a `MenuEvent` on the main thread (`route::chosen`) and sends it as
an `Action`, `{ action: 'command', command }`, through the `Channel` the page
handed over with `listen` once its screens were drawn. The page a lock builds
next hands over its own, which replaces it, and Rust lets go of it in the
`Destroyed` arm of the run callback. A page is known by its window's label, so
something the last page sent that lands after the next one is listening is not
taken for the new page's, and a window's `Destroyed` handled after the next
page listened leaves that page and its bar alone. Each screen says what it
answers (`answer` in `menu.svelte.ts`). No two screens answer the same item at
once yet - the title bar's Settings… is offered only where the vault's status
bar is not drawn - and when two do, the newest that can is the one that does,
because a screen drawn over another was drawn after it.

A choice runs the way a press on its button would. A sheet over the window goes
first. Then the field being written in is left: a press on a button takes the
focus out of the field before the button runs, and leaving is when a field
writes what was typed into it, while a choice from the menu bar takes no focus.
Without it New Entry took the field off the screen with the value still in it -
WebKit tells nothing it removes that it lost the focus - and the value was
written only by the next lock. Keyboard Shortcuts leaves the field alone: its
sheet takes the focus and gives it back. So does the search field
(`role="searchbox"`), whose leaving writes nothing: a reader who copies a login
from the menu while searching goes on typing in it.

What was chosen happens in the window, so the window comes forward for it
(`window::bring_back`). A minimised window still has its page listening and the
bar as that page left it, and AppKit hands the bar its keys with no window in
front: Move to Recycle Bin would otherwise run out of sight, and its offer to
undo run out unread. Lock Vault alone is left where it is, because it takes the
window down and the one it builds comes forward asking for the password.

**The page says what applies; the session has the last word on two.** Whenever
the items its screens can do change - each on the condition its button is drawn
on - the page sends `menu_state` with the list (`greying.ts`). One message at a
time: Rust answers two side by side and in either order, so the bar would
otherwise be left as whichever finished last said. Rust filters the list through
the session - Lock Vault only with a vault open, Open Vault… never then, because
it refuses to point the session at another file while one is - reading whether
one is open without waiting for the session, which a save holds for seconds: a
report stuck behind it left an item grey that applied, and AppKit drops the key
of a grey item. It greys the rest out on the main thread, by a message posted
there rather than waited for. A choice that crossed with a change on its way is
asked again when it arrives, and one nothing can do any more does nothing - it
does not even put a sheet away. Open Vault… applies wherever no vault is open:
on the unlock screen and the creation screen - except while a key is derived
there or a vault made, because a file picked then would make the work in flight
land stale - and on the settings with nothing open. With no page listening only
Open Vault… and Settings… can be chosen. Either builds the window again
(`window::bring_back`), and the choice waits in Rust for the page that arrives:
the reader closed the window and went to the menu bar to get it back. Only the
last such choice waits. Whether there is a page and keeping the choice for the
next are decided under one lock, so a choice made just as a page starts
listening reaches that page rather than the window after it; a choice told to a
page, or one whose send failed, is never kept.

**A key the menu owns is not the page's.** WebKit hands the page a key before
AppKit looks for it in the menu, and a key the page answered is gone, so a key
both answered would happen twice. The page's `keydown` answers Escape, Cmd+Z
(the notice's undo, see above), Cmd+C with nothing selected (the open entry's
password, as the mockup has it) and Cmd+A with the focus on neither a field nor
the entry pane (every row the list draws, see `docs/design.md`), and nothing
else; every other Cmd key is the menu's, and works wherever the focus is. Cmd+B
copies the open entry's login with the focus in a field too, where the page used
to leave it to the field: it is a copy, through Rust, and the field is left
first, as above. A login being changed in its own field is written, and the copy
waits until every value on its way to Rust has arrived (`flush` in `drafts.ts`)
before it is asked for: Tauri answers the two side by side, and once a save lets
go the session goes to whichever asks first, so a copy sent beside the write
could put the login as it was on the pasteboard. Copy Login is offered on the
condition the row's copy button is drawn on - a login Rust holds - so one typed
into an empty field is offered once that field is left. Copy Password is on
Shift+Cmd+C because Cmd+C is Edit ▸ Copy, which is how every text field in the
window copies: an item of Coffer's on it would take copying away from them all.
Cmd+Backspace in a field deletes to the start of the line, so Move to Recycle
Bin is greyed out while a field has the focus, and it is offered only for an
entry whose deletion goes to the bin, which is what it says; one that goes for
good asks first, from the pane. While rows are chosen in the list it moves them
rather than the open entry, offered only when every one of them goes to the
bin; a choice holding one that would go for good asks first, from its bar.

**Closing the window locks, and Coffer waits in the Dock.** Before this the
close button quit Coffer: the last window going asks the loop to exit
(`ExitRequested` with no code), and nothing said no. Now the close is asked of
the page first. `CloseRequested` sends `{ action: 'closing' }` and holds the
window up; the page sends what is being typed, the way the Lock button does, and
asks for `close_window`, which locks with `Reason::Closed` and takes the window
down without building it again. A page that has not asked within three seconds
is closed from Rust all the same - a close JavaScript can hold up is a lock
JavaScript can refuse - and with no page listening the window goes at once and
the lock runs off the main thread. The three seconds run only while nothing
holds the session (`Session::busy`): the page's drafts wait behind a save for
it, and a lock from Rust that took it first - the session is not handed out in
the order it was asked for - would write the vault without what the reader
typed during the save. The lock goes through the timer like every
other, and then straight to the session as well, so that an unlock still
deriving its key lands after it and is refused as stale rather than opening a
vault behind a window that has gone. `ExitRequested` with no code is refused
from then on. Quit never asks it - `terminate:` ends the loop and arrives as
`Exit`, which locks with `Reason::Quitting` as before - and an exit with a code
is one Coffer asked for itself. A click on the Dock icon (`RunEvent::Reopen`)
brings the window back: forward, and out of the Dock when it was minimised, or
built again where it was left when the reader had closed it. The unlock screen
it comes back to says nothing about why: the reader asked.

## The master password

It reaches Rust as the whole body of the message, as bytes:

```ts
await invoke('unlock', new TextEncoder().encode(password));
```

A `Uint8Array` as the second argument is what makes Tauri send
`application/octet-stream`, which arrives as `InvokeBody::Raw`. Anything else -
an object with the bytes inside it, a string - is serialised as JSON, and the
password would exist as text in a JavaScript string and in a JSON document.

`unlock` therefore **refuses a JSON body** rather than accepting the number
array Tauri would fall back to. The fallback is real: if the
Content-Security-Policy does not allow `connect-src ipc:`, the webview's fetch
to `ipc://localhost` is blocked, Tauri logs one warning and resends the same
message through `postMessage`, where `JSON.stringify` turns the bytes into
numbers. Refusing is what keeps a broken policy from becoming a silent leak.

**The copy Tauri keeps, and the half of it that is now closed.** WebKit copies
the request body once, Tauri moves it into an `InvokeBody::Raw(Vec<u8>)`, and
that buffer is dropped without being wiped. `Zeroizing` on this side covers only
the copy Coffer takes. Since slice 4 the Rust half of that residue is covered
too: `InvokeBody::Raw` holds an ordinary `Vec`, and
[`vault_core::scrub`](../crates/vault-core/src/scrub.rs) writes over every block
this process frees. What is still out of reach is WebKit's own copy, which is in
another process, and anything Objective-C allocated - the pasteboard's string
included, which is why the clipboard is cleared on a timer rather than trusted
to a wipe.

## Why the shape of a command is what it is

**`unlock` and `choose_database` are `async`.** A plain `#[tauri::command]` runs
inline on the thread that handles IPC, which on macOS is the thread that draws
the window. Key derivation there freezes the window for a second, and a file
dialog there deadlocks outright: the panel needs the run loop that the call is
blocking. `unlock` goes further and pushes the derivation onto a blocking
thread, so a tokio worker is not held for a second either.

**A raw body cannot be combined with named arguments.** Tauri's argument
deserialiser fails on every call the moment a command asks for both. That is why
the database is chosen by a command of its own and remembered in Rust, rather
than being sent alongside the password.

**The session is behind an `Arc`.** Locking is `Option::take` on the vault
inside it: `Manager::unmanage` is deprecated and documented as unsafe, and it
would dangle every reference already handed out.

## Capabilities

`capabilities/main.json` grants the main window two window permissions and
nothing else: `core:window:allow-start-dragging`, because the title bar is drawn
in HTML and has to ask the window manager to move the window, and
`core:window:allow-internal-toggle-maximize`, because a double click on a title
bar zooms a window on macOS.

Coffer's own commands are **not** gated by that file. Tauri's ACL applies to
plugin commands, and application commands from a local origin bypass it unless
the crate declares an ACL manifest of its own. The gate on `unlock` is that it
is `unlock`, not that a capability lists it. Adding a `permissions/` directory
to this crate would flip that and every command would then need an entry.
`listen` takes a `Channel`, which needs nothing here either: the channel's own
command, which a page uses to fetch a message too large to evaluate, is let past
the ACL by name (`src/webview/mod.rs` in tauri 2.11.5).

The dialog plugin is registered for its Rust API only. Its three webview-facing
commands - `open`, `save`, `message` - are left unpermitted, so the file picker
cannot be opened from the webview at all.

## The Content-Security-Policy

`app.security.csp` in `tauri.conf.json` is the only policy. SvelteKit's own
`kit.csp` is left alone: two policies are enforced as their intersection, and
Tauri already hashes every inline script it serves at build time, which is what
lets SvelteKit's bootstrap script run under `script-src 'self'`.

Two things follow from that, and both are traps:

- **The policy is not enforced in `tauri dev`.** With `devUrl` set, the window
  loads the Vite server directly and Tauri's asset handler, which is what adds
  the header, never runs. A policy bug only appears in a real build.
- **Every bar that drains is a CSS animation of a fixed length, and the lengths
  are written twice.** `--animate-drain-reveal` in `app.css` is the thirty
  seconds of `SECONDS` in `reveal.svelte.ts`. The clipboard's is one animation
  per timeout `settings.rs` offers, because the reader chooses it: `drain.test.ts`
  reads both files and fails when the two lists disagree, which is as close to
  one place as a stylesheet and a Rust constant can get. Neither pair can share
  a value, because the only way to set a duration from script is an inline style
  and the policy below forbids one.
- **The countdown's bar and the calibration's bar are widths, not animations.**
  A width cannot be computed into a class either, so both pick from a short
  table of literal ones. Twelve steps is as fine as a sixty-four pixel bar
  shows.
- **Nothing in the window may use an inline style.** `style-src 'self'` covers
  `style` attributes as well as `<style>` elements, and Tauri's nonce only
  reaches elements that are in the HTML at build time. This is why the sprite is
  hidden with a class, and why the two timers that drain on screen are CSS
  animations of a fixed length rather than a width that JavaScript sets.
- **A message through a channel is not governed by the policy.** Rust evaluates
  it into the page with `evaluateJavaScript:` (`src/wkwebview/mod.rs` in wry
  0.55.1), the route Tauri's own answers take, and every one Coffer sends is far
  under the 8 KiB above which Tauri parks it in Rust and has the page fetch it
  over `ipc:` instead.

## Locking

Locking destroys the window. It does not hide one: a hidden window is a webview
still holding every value the reader looked at, in a heap nothing in this
process can reach.

One route in, whatever asked. The idle deadline, the machine's own
notifications, the button and the close button all post to
[`autolock::timer`](../crates/vault-gui/src/autolock/timer.rs), which is what
calls [`lock.rs`](../crates/vault-gui/src/lock.rs); a close calls it once more
after the timer, for an unlock still in flight (see The menu bar). That order is
the whole of what `lock.rs` is: the tree is wiped **first and synchronously**,
because destroying a window is a message to the event loop and a Mac going to
sleep will not wait for it; then the clipboard is taken back; then the stack the
key was derived on is written over; and only then is the destroy queued, with
the window asked back unless Coffer is quitting or the reader closed it
(`Reason::comes_back`).

`Session::lock` answers whether it actually dropped a vault, and exactly one
caller is told yes. Tauri goes on handing out the window between a destroy being
queued and the event that says it happened, so a second trigger arriving at the
same moment would otherwise destroy the window the first one's rebuild had just
made.

After a lock the window is built again in the `Destroyed` event of the run
callback and nowhere else. A destroy followed by a build in one function always
fails: the label is taken until the event is delivered. After the reader closed
it, it is built on `Reopen` or a choice in the menu bar (`window::bring_back`).
Every build after the first goes through `window::again`, under a label no
window has carried, and from its entry in `tauri.conf.json` rather than by hand,
so the rebuilt window is the same window, and it is put back where the reader
left it.

**No Tauri event goes the other way.** The rebuilt page asks `status` and gets
the reason it is asking for a password again. An event would need two
capabilities the window does not have, and Tauri never clears a destroyed
window's listeners: a rebuilt window reuses the label `main`, so every emit
after the first relock would serialise every dead listener id ever registered.

One channel does. The menu bar's choices and the close button reach the page
through the `Channel` it hands over with `listen` (see The menu bar). It needs
no capability, and nothing of it outlives the window: Rust lets go of it in the
`Destroyed` arm, and the next page hands over its own.

## What the window does not do yet

**It draws every row it is given.** The list has no windowing: a vault of a
thousand entries, which is what `SPEC.md` sets a time budget for, draws in one
pass and filters on the text it folded when the folder was opened. Fifty
thousand would draw fifty thousand rows and take its time about it. The filter
itself is tested at that size; the drawing is not, and the fix when it matters is
to draw only the rows on screen. Cmd+A over fifty thousand rows redraws every
one of them on the selection plane, and that is not measured either.

**Rows are chosen with the pointer and Cmd+A.** The list has no arrow keys and
no focus of its own that moves from row to row, so there is no Shift+arrow and
no Space to choose with; that belongs with the keyboard work on the list rather
than with choosing.

**The command layer itself has no tests.** What is under it does:
`Session` is covered, `vault-core` is covered, and `contract.test.ts` checks
that every command the window calls exists in Rust, is in the handler list, and
takes exactly the arguments the window sends. What is untested is the body of a
command - the dialog it opens, the thread it moves work onto. Testing one needs
Tauri's mock runtime and belongs with the window work rather than with this
slice.

**A folder is moved with the pointer only.** An entry moves from the line above
its title, which the keyboard reaches like any other button; a folder has no
such line, and is dragged onto another in the folders pane. A reader without a
pointer cannot move one yet.

**The three window buttons are not moved at all.** macOS puts the close,
minimise and zoom buttons a fixed distance below the top of the window, and it
lays them out again on every pass - which, while a window is being dragged, is
every frame the display draws. Every version of this that moved them afterwards
flickered, and had to: a correction that answers a notification, an event or a
timer is one that sometimes lands after the frame it belonged to, and each of
those is a frame drawn with the row where AppKit put it.

So [`buttons.rs`](../crates/vault-gui/src/buttons.rs) changes the view the
buttons are in instead. AppKit writes the row's place in the coordinates of
whichever view holds them, and it writes the same two numbers every time: an
offset from that view's left edge, and an offset up from its bottom. The three
buttons are taken into a view of Coffer's own, sized so that those two numbers
land at `px-4` from the left and in the middle of the `h-11` title bar. AppKit
then goes on placing the row as often as it likes and every placement is already
right, so there is nothing to correct and no moment at which a correction can be
late. While the window is dragged the view follows the top of it on a flexible
bottom margin, which AppKit applies inside the call that resizes the window
rather than in answer to it.

The view answers a click only where a button is, because the reader drags the
window by the title bar beside them.

AppKit takes the row back sometimes - for a full screen, and in the setup that
follows a window being built, which a lock showed as a row back where macOS
wanted it. There is no notification for a view changing hands and none is needed:
the view a subview is leaving is asked first, and that view is Coffer's, so
`willRemoveSubview:` is where the row is asked for again. It is asked on the next
turn of the run loop rather than there and then, because AppKit is in the middle
of the move. The same ask runs once after a window is built, and on a resize, a
scale change and a window coming forward - the three events that mean AppKit has
had the window in its hands. Every one of them is free when there is nothing to
do: a check that finds the row in place does not touch it, which is what makes
them safe to answer while a window is being dragged.

**The window is shown when its page has drawn, not when it is built.** A webview
is white until its first frame, whatever the window around it is wearing and
whatever `app.html` says about its colour scheme, and a lock is the one moment a
whole window is built while the reader watches. So `visible` is false in the
window Rust builds and the page's own load event is what shows it. The cost is a
beat with no window during a lock, where there was a beat of white before; the
risk is a page that never finishes loading, which is a dev server that is not
running and a window that does not appear rather than one that is blank.

**A save holds the session while it runs.** Every committed change writes the
file, and the write derives the key again, so for that second nothing else can
read the vault. The status bar says `Saving…`. If that ever becomes a wait
worth avoiding, the answer is to hold the write behind a short delay rather than
to let two of them overlap.

**A snapshot opened from the unlock screen becomes the chosen database for the
rest of the session,** and it is read only. Writing to a `.bak` would put the
change in a file the next save of the database beside it rotates away, so every
change is refused with `readOnly` and `save_copy` is the way out: it writes what
is in the window to a file of the reader's choosing, which then opens like any
other database.

**A lock writes the vault out before it wipes it.** A vault is dirty exactly
when saving is what failed, so locking on its own would be a session's work
ended by a timer. What the reader was typing and had not finished goes in first,
through the same edit leaving each field would have made, so that it is written
out with the rest. The ordinary save is tried first; what it will not take goes to
`<database>.unsaved.kdbx` beside the vault, under the same credentials. The lock
happens either way, and `status` reports the copy as `rescue` and a rescue that
could not be written anywhere as `lost`. The copy stays until the reader removes
it through `discard_rescue` or makes it the vault again; Coffer never removes it
on its own.

**The copy becomes the vault again in one of two ways, and the paths are Rust's
in both.** Which vault a copy belongs to is read off its name - the vault's
file name with `.unsaved.kdbx` after it - and nothing else records it. `status`
answers `file`, the chosen file as the disk has it (`there`, and `written` when
the filesystem keeps a time), and, when the chosen file is a lock's copy,
`copy`: the vault's file name, when the copy was written, the name the vault's
file will be kept under (`keptAs`, the newest snapshot's, which later saves
push out like any other), and `vaultFile`, the vault's own file as it stands.
Names and times; no path crosses. Rust keeps how the vault's file stood when it
said so, and `promote_rescue` is held to it.

- **The vault's file has gone.** `put_back_rescue` moves the copy into the
  vault's name without a password, because nothing is opened. The copy's bytes
  go into a temporary file of Coffer's own beside the name, owner-only and
  flushed, and that file is then given the vault's name with a hard link, which
  is refused if anything at all is at the name by then. So nothing is ever at
  the vault's name that is not the whole copy, a file that arrives while it
  runs - the vault dragged back from the Trash, a sync client catching up - is
  never written over or removed and the answer is `taken`, and a process killed
  part way leaves the name empty and the copy where it was. The copy is removed
  only once the vault's name holds all of it, and the vault is born owner-only.
  A copy that has gone answers `gone` and leaves nothing at the vault's name. A
  disk that keeps no second name for a file (FAT, exFAT, some shares) answers
  `needsOpening`: there is no way there to put a whole file at a name that is
  refused when something arrived first, so the copy is opened and made the
  vault from inside, with the ordinary write.
- **The vault's file is there.** The copy is opened to be looked at, with
  `choose_rescue` and the vault's password, and the key file chosen for the vault
  goes with it. Everything changed inside it changes the copy, and the window says
  so across the top. `promote_rescue` writes what is open over the vault's file
  the way `save_over` does - the file as it stands becomes `<vault>.1.bak` first -
  removes the copy only after that write went through, and leaves the session
  open on the vault, which is then the vault written down for the next launch.
  The vault's file has to stand as `status` last said it did: another client's
  save since, or a vault that went or came back, is refused with
  `externalChange` and nothing written, and the window reads `status` again so
  that the banner says how it stands now before the reader presses again. A
  refusal or a failure before the write lands leaves both files as they were,
  and the copy still open. `leave_rescue` goes back instead: with the copy open
  it locks, so what the copy holds is written into the copy on the way out.
  Only when that lock kept nothing beside the copy - nothing to write, or the
  copy took it - is the session pointed at the vault, and the window that comes
  back asks for the vault's password. When the copy's own save failed and the
  lock put its work in a copy of the copy, or could put it nowhere, the session
  stays on the copy, whose unlock screen offers that copy or says what was lost
  and what is left of the copy. `typed` and `lost` are about the file the lock
  closed and are never carried to the vault's screen. With nothing open it only
  points the unlock screen back, key file and all.

Both moves hold the lock file beside the vault's name and the one beside the
copy while they run, and refuse with `heldByAnother` when somebody else holds
either. A move leaves no note behind: the copy's goes with the copy, and the
vault's stays only while the vault is open, like any vault's. Both answer
`readOnly` where the folder will not take one.

A copy's own snapshots go with it. Every save made inside an open copy rotates
`<copy>.1.bak` and on beside it, as any save does. Once the copy is made the
vault, put back or removed with `discard_rescue`, those slots are removed as
well: nothing lists a copy's snapshots once the copy is not there to choose, so
they would sit in the folder unseen, as earlier states of the vault that open
with whatever password it had then - after the reader changed it because they
believed it leaked - and the next lock's copy would rotate into the stale chain.
What the copy replaced is in the vault's own chain, which is never touched.

A password-free move over a vault that is there is deliberately not offered,
even when the vault's modification time is older than the copy. The usual reason
a lock needed a copy is that another client had already rewritten the vault,
before the copy was made, and a rule by time would quietly push that client's
work into a snapshot. Every other way a lock writes a copy while the vault's
file is unchanged - the file made read only, a snapshot slot that would not
rotate, a disk that failed the write - is the vault's file refusing a write,
and a move that stepped round it would step round the very check that refused.
The copy is opened and looked at first, and the write that replaces the vault is
the ordinary one, with its snapshot behind it.

When a lock could write nothing at all, `lost` is said with what survived: the
chosen file's `written`, or that it has gone as well.

**A vault kept in a place that will not take a file opens read only.** A vault
on a read-only disk image, inside a Time Machine snapshot, on a stick macOS
mounted read-only or on a share the reader may only read cannot have a lock file
written beside it, and used not to open at all. It opens, `read_only` is true in
`status`, every change answers `readOnly`, and `save_copy` writes what is in the
window somewhere the reader can write.

## Where this departs from SPEC.md

**A copied password is kept off Universal Clipboard.** `SPEC.md` says macOS
exposes no documented way to exclude an item, and that this has to be admitted in
the README. It does:
`prepareForNewContentsWithOptions(NSPasteboardContentsOptions::CurrentHostOnly)`
clears the pasteboard exactly like `clearContents` and marks the write as
belonging to this host, so Handoff does not carry the value to another device.
Coffer uses it. The README promise can be dropped when it is written.

**The clipboard clears itself in slice 2.** The timer belongs to slice 4, which
makes it configurable, but the copy toast in `design.html` promises a clipboard
that empties itself, and a toast that promises what the application does not do
is worse than an early timer. Sixty seconds, the spec's own default, with the
change count checked before the clear and the contents never read.

**The Tauri crate is `crates/vault-gui`, not `src-tauri`.** `SPEC.md` names
`src-tauri/capabilities/` when it describes capabilities, which is the framework's
default layout. Coffer has one workspace with two crates, as the same document
says two paragraphs earlier, so the capability files, the configuration and the
window icon live in the crate that runs `tauri_build::build()`.

**TypeScript needs two majors installed.** `svelte-check` will not run against
TypeScript 7 unless 6 is installed beside it and the check is given `--tsgo`,
which is what it says to do and what `package.json` does. `@typescript/native` is
the alias that carries 7; `knip.json` names it, because nothing imports it.

## Running it

```bash
cd crates/vault-gui
cargo tauri dev                                  # starts vite on 1420
cargo tauri build --target aarch64-apple-darwin  # a .app and a .dmg
cargo tauri build --target x86_64-apple-darwin   # the other architecture
```

There is no universal binary. Merging both halves with `lipo` doubles the weight
for nothing the reader notices, so the two architectures are two builds and two
release assets, and `install.sh` picks between them on `uname -m`.

The icon files under `crates/vault-gui/icons` are generated from the brand mark
by `assets/appicon.sh` and committed, because CI has no SVG renderer and
`generate_context!` reads one of them at compile time. `icon.icns` is what the
bundle ships and what the dmg wears as its volume icon. `icon.png` is 128 squares
because macOS never draws a window icon and the file is compiled into the binary
as raw pixels, so a 1024 square would cost four megabytes for nothing.

The mark is drawn into the middle 824 of a 1024 square. macOS masks nothing and
scales an icon to whatever size the file gives it, so an icon drawn to the edge
stands a quarter wider than everything beside it in the Dock.

**The bundle declares macOS 13.3, not 12.** Tailwind v4 compiles `color-mix` and
`@property` into the stylesheet, and Safari 16.4 is the first release that reads
both. Below it six utilities fall back to a colour literal frozen at build time,
which is the dark one - so on an older Mac the light theme would paint a dark
scrim, dark row hovers and dark focus rings. Declaring the floor is cheaper than
carrying a second stylesheet for a version of macOS that is three years old.
