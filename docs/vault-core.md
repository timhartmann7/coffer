# The vault engine

`vault-core` opens a KDBX file, hands back the tree, hands back one value at a
time, applies a change, and saves with a snapshot behind it. It knows nothing
about Tauri, a window or a webview.

Everything below is what a reader of the source cannot work out from the source:
what the library underneath does, where Coffer works around it, and which
hazards are waiting for the slices that come next.

---

## The library underneath

Coffer is built on `keepass`, pinned to exactly `=0.13.25`. The pin is not
caution: `0.13.x` has shipped breaking refactors inside patch releases. Bump it
by hand and run the round-trip suite.

Only the `save_kdbx4` feature is enabled. `totp` is out of scope, and
`challenge_response` pulls in USB access, does not compile against the current
`nusb`, and buys nothing: reading a hardware-key database needs the hardware.

### What the library gets wrong, and what Coffer does about it

Line references are into
`~/.cargo/registry/src/index.crates.io-*/keepass-0.13.25/`.

**Every KDBX 3 attachment collapses onto one.**
`format/xml_db/mod.rs:108-123` assigns each `Meta/Binaries` entry
`AttachmentId::next_free(&db)` against a `Database` whose attachment map is
still empty, so every one of them gets id 0 and all but the last is destroyed
at load. Entries referring to the others lose the reference silently.
*Coffer refuses to save a KDBX 3 database that has any attachment*
(`VaultError::ReadOnlyKdbx3Attachments`). It can still be read; it cannot be
written back, because writing it back would write fewer attachments than were
read.

**An empty KDBX 3 attachment cannot be read at all.**
`format/xml_db/meta.rs` gives `Binary.value` no `#[serde(default)]`, so a
zero-byte attachment in `Meta/Binaries` fails deserialisation outright. Coffer
reports `VaultError::DamagedContent`.
`legacy-empty-attachment-kdbx31.kdbx` pins the behaviour so that the day the
library learns to read one, a test fails and this paragraph comes out.

**Attachment references are bound by position on read and by identifier on
write.** `format/xml_db/mod.rs:67-77` writes the inner-header binaries sorted by
`AttachmentId`, while the entry XML carries the identifier itself, and
`xml_to_db` assigns identifiers by position in the header. Contiguous
identifiers make the two agree. **A gap does not**, and every attachment after
the gap is rebound to the wrong entry or dropped.
Nothing in slice 1 can create a gap, because nothing in slice 1 removes an
attachment. **Slice 3 must not ship attachment removal without a check that the
identifier space is still `0..n`.**

**Attachment back-references are never rebuilt on load.**
`Attachment::entries` is populated only by `add_attachment`, so after
`Database::open` every set is empty. `remove_attachment_by_name` reads those
sets to decide whether a blob is still referenced, concludes it is not, and
deletes a blob that other entries and history versions still point at. Another
one for slice 3.

**`edit_tracking` writes a version whether or not anything changed.**
`db/types/entry.rs:831-841` pushes on `Drop` unconditionally, and every setter
on `EntryTrack` stamps `times.last_modification` on the way through, so the
pre-edit clone can never compare equal by itself. `history::edit` compares the
entry against the version the library just pushed with the timestamps and the
nested history taken out, and when they match it drops the version and puts the
old timestamps back. An edit that changed nothing did not happen.

**History has two contradictory orders.** A file written by KeePassXC lists
versions oldest first and the parser preserves that order, while
`History::add_entry` inserts at the front. Position therefore means nothing.
`history::prune` sorts by `last_modification` and rebuilds oldest first, which
is what KeePassXC writes, so a database Coffer saved looks like one KeePassXC
saved. The rebuild is the only way to prune at all: `History::entries` is
private and `add_entry` is the only way in. The order itself - by
`last_modification`, a version nobody dated after every dated one - is
`history::age`, and the version list, the prune and the question below all sort
by it, so none of them can call a different version the newest.

**A position holds only until the next edit.** `Vault::edits` is a number that
moves on every change the vault marks itself changed for, on every write, whose
settling prunes and re-sorts, and on every reload - and on nothing else, so a
change the vault refused or found nothing to do in leaves it where it was.
`vault-gui` compares it to tell whether a version's position read earlier still
names the same version.

**Taking a removal back is a restore, and only of one version.** Removing a
field writes a version like any other edit, and restoring that version is the
undo. But the save after the removal prunes, and what is newest after a prune,
a change made since, or a version another client dated later is something else,
whose restore would take back every change since as well. `Vault::undo_removal`
restores the newest version only when it is the entry as it stands with that
field put back, and refuses with `RemovalSuperseded` otherwise - never the
position of some other version that happens to hold a field of that name. The
question and the restore are one call.

**A removal whose version the save would drop asks first.** A database that
keeps no versions, or a size limit the removal's version does not fit, drops
that version at the very next save, and the field is gone for good.
`Vault::remove_field` refuses that removal with `RemovalForGood` unless it is
told the reader agreed. It decides by putting the version the removal would
write through `history::keep`, the rule the save prunes by, beside the versions
the entry already has - so the question and the save cannot disagree about
which versions a save keeps.

**Key derivation runs on unauthenticated parameters.** `format/kdbx4/parse.rs`
derives the key from the KDF dictionary in the outer header before it checks the
header HMAC, so a file claiming four terabytes of Argon2 memory takes the
process down before anybody has proved the file is genuine. `preflight.rs` reads
the same bytes first, with no indexing and no arithmetic that can wrap, and
refuses anything past its ceilings.

**An unknown child element inside `<Group>` aborts the whole parse.**
`format/xml_db/group.rs:66` collects unmatched children into
`Vec<GroupOrEntry>`, which has no other variant, so a `<Group>` child element
the library does not know is a hard parse failure rather than something ignored.
If a future KeePassXC adds one, databases carrying it will not open in Coffer
until the library catches up. Unknown elements elsewhere are silently discarded
instead, which is the residual risk `SPEC.md` already names.

**Derived `Debug` prints secrets.** `Database`, `Entry`, `Group`, `Meta`,
`Attachment` and `DatabaseKey` all derive `Debug` over plaintext, including the
master password. Nothing in `vault-core` may hold one of those types in a struct
that derives `Debug`, and nothing may format one.

**A field value that is nothing but whitespace comes back empty.** An
unprotected `<Value>   </Value>` is read as the empty string, so the three
spaces are gone before Coffer has the database and saving writes the loss back.
Whitespace *around* a value survives, and so does a whitespace-only protected
value, because a protected value is base64 and its XML text is not whitespace.
`whitespace-kdbx41.kdbx` pins all three, so the day the library keeps them a
test fails.

**A KDBX 4 file that puts its attachments in `Meta/Binaries` hits the same
collapse as a KDBX 3 one.** The parser reads that element whatever the format
version says, so the read-only guard, which keys on the version, does not catch
it. Telling the two apart needs the decrypted XML, and the only way to get it is
`Database::get_xml`, which derives the key a second time and would double the
time to unlock every database that has an attachment. KeePass 2 and KeePassXC
put KDBX 4 attachments in the inner header and never write that element, so the
file this would take is one no released client produces.

**Two hazards Coffer cannot reach and cannot fix.** Timestamp deserialisation
panics on a base64 payload shorter than eight bytes, and the gzip payload is
decompressed with an unbounded `read_to_end`. Both are inside the encrypted
body, so they need a valid key, which means a database the user already has. The
file-size ceiling in `vault.rs` bounds the input; the decompressed size is not
bounded. The fuzz target in `fuzz/` exists to find the rest.

---

## Measuring coverage

`cargo llvm-cov nextest --workspace` cannot merge the profile of a test whose
subprocess aborts on purpose, and two of them do. Exclude those two:

```
cargo llvm-cov nextest --workspace --summary-only \
    -E 'not (test(killing_the_process) + test(a_save_that_runs_out_of_room))'
```

## Compiling it for the machine CI runs on

This crate is the half that is not macOS, and CI proves that by building it on
Ubuntu. A Mac cannot see a Linux-only break: the memory scan in
`tests/vault/scan.rs` has a block per platform, and an import that only one of
them needs compiles cleanly on the platform that needs it and fails on the other
under `#![deny(unused_imports)]`. That failure has nothing to do with the change
that provoked it and everything to do with where it was compiled.

So before pushing anything that touches this crate, run the engine job's own
check against its own target:

```
cargo clippy -p vault-core --all-targets --all-features \
    --target x86_64-unknown-linux-gnu -- -D warnings
```

`rustup target add x86_64-unknown-linux-gnu` once, and it needs no linker: a
check compiles without linking, which is enough to find every conditional
compilation mistake this crate can make.

That catches what will not compile. It does not catch what compiles and then
behaves differently, and this crate has had three of those. Two turned CI red
after a green run on a Mac: Linux reads a file's timestamps from a clock that
only moves once a timer tick, so two watcher tests saw no change where a Mac
sees one, and `xattr` and `touch` are not where a Mac keeps them, so a helper
naming their paths panicked. The third never went red at all - a container has
neither `USER` nor `LOGNAME`, so the lock test compared the empty string it had
computed against the empty string the engine returned, and passed having
checked nothing.

That last one is the reason to run the suite here rather than only read it: a
Linux machine finds assertions that are vacuous as well as ones that fail.

```
docker build -t coffer-engine -f docs/engine.Dockerfile .
docker volume create coffer-target
docker run --rm -v "$PWD:/src:ro" -v coffer-target:/home/runner/target \
    -e CARGO_TARGET_DIR=/home/runner/target coffer-engine \
    sh -c 'cd /src && cargo test -p vault-core --all-features --locked \
        -- --skip too_large_to_open_again'
```

The source is mounted read-only and the target directory is a named volume, so
a run cannot touch the working tree and a second run does not recompile the
world. `-p vault-core`, never `--workspace`: `vault-gui` is macOS and will not
build here. The skipped test is
`a_database_that_would_be_too_large_to_open_again_is_not_written`, which wants
more memory than Docker Desktop gives its VM by default and is killed rather
than failed; it is left to CI, which has enough.

## Where Coffer departs from SPEC.md

**The master password lives as long as the vault does.** `SPEC.md` says it is
"zeroed right after key derivation". KeePass derives a fresh key on every save,
so a vault that can save is a vault that still has the password. It sits in a
`Zeroizing<Vec<u8>>` inside `Vault` and is wiped when the vault is dropped,
which is what locking has to mean. Changing it replaces it there, and the old
one is wiped as soon as the file holds the new key.

**KeePassXC 2.7.12 writes no per-database lock file.** `SPEC.md` says it does
and asks Coffer to follow the convention. The claim no longer holds: the
application binary contains no `.lock` literal, and its only `QLockFile` use is
the single-instance lock, whose strings are "Existing single-instance lock file
is invalid" and "The lock file could not be created. Single-instance mode
disabled". Coffer still writes `<database>.lock` in the INI shape KeePass 2.x
uses, so a KeePass 2.x client recognises it, but in practice it guards Coffer
against Coffer. It is advisory in every case.

**A vault kept where nothing can be written opens with no lock file at all.**
`SPEC.md` says "read someone else's lock and warn, write our own, remove it on
close", and the middle of those cannot happen on a read-only disk image, inside
a Time Machine snapshot, on a stick macOS mounted read-only or on a share the
reader may only read. Reading a database needs no write, so a vault Coffer
refused to open over a note it could not leave was refusing a file that reads
perfectly well. It opens, `read_only()` is `Some(ReadOnly::Place)`, every change answers
`ReadOnlyPlace`, and a copy to somewhere writable is the way off the
medium. A lock somebody is holding still wins: the medium is only consulted
where the file could not be created at all.

**KeePassXC writes KDBX 4.0 and downgrades files Coffer wrote.** `dump_kdbx4`
refuses any version but 4.1, so Coffer always writes 4.1; KeePassXC 2.7.12
writes the lowest version the content needs, which is usually 4.0. A file
alternately saved by both flips between the two. Both are KDBX 4 and both read
either way.

**Saving upgrades a `Plain` inner cipher to ChaCha20.** A database declaring no
inner cipher stores every protected value as base64 plaintext inside the
encrypted body. Coffer writes the stronger one. It is not a field, so nothing is
lost, and the exported XML is identical either way.

**A group's empty `<Notes/>` comes back absent.** The library's optional-string
helper collapses an empty element to `None`, and the writer omits it. KeePassXC
writes the empty element for both, so the round trip cannot tell them apart and
nothing is lost through it. Entry field values are a different path and an empty
one there stays empty.

**Golden fixtures come from one KeePassXC version.** `SPEC.md` asks for several.
2.7.12 is the only version installable: the beta cask is the same build and the
snapshot cask fails Gatekeeper. `tests/fixtures/generate.sh` is
version-agnostic, so a second set lands beside the first by installing another
version and rerunning it.

**A file the pool cannot close up over is not removed.** Taking a file away
means every file above it moves down a slot, and a file that has to move needs
every name for it rewritten - which cannot be done for a name inside a previous
version, and cannot be done twice for a file that two entries name. Both are
shapes another client produces and Coffer cannot, so both are refusals rather
than corruption: `AttachmentPinned`, with the file left exactly where it was.

**A file that only a previous version names stays in the database.** Removing an
attachment removes the one that was asked for. A file left with no name at all -
which a database written by another client can arrive with, and which clearing an
entry's history can leave behind - is kept: the pool cannot hold bytes without a
name, so closing up over it would drop it, and dropping something a file arrived
with is what the first rule forbids.

**A save that would produce a file too large to open again is refused.** The
whole database is read into memory to be opened, so the ceiling on that is a
ceiling on what may be written. It is measured on the encrypted bytes as they
are staged, where the database they would replace is still untouched, because
the payload is compressed and nothing about the tree in memory predicts the size
of the file.

**A database Coffer saves says `Coffer` in `Meta/Generator`.** Every KeePass
client writes its own name there and the field is the writer's signature rather
than anything of the user's, so the round-trip suite normalises it away.

**`VaultError::ReadOnlyKdb` has no test.** No available tool writes a KeePass 1
file: KeePassXC 2.7 dropped the writer and the CLI never had it. The guard stays
because saving a KDB-sourced database as KDBX 4.1 is exactly the kind of silent
conversion the first hard rule exists to prevent.

---

## What the slices after this one must take from here rather than invent

**The URL scheme allowlist is written.** It is `vault_core::url::openable`:
`http`, `https`, `mailto` and `ftp`, compared after the value has been trimmed
and refused outright when a control character sits inside it. `vault-core` hands
URL values back exactly as the file holds them, leading whitespace and mixed case
included - `open::dangerous_url_schemes_are_handed_back_unchanged` pins that -
which is why the check normalises before it compares and hands back the string it
judged. `vault-gui::opener` is its only caller, and the entry screen asks it
rather than deciding for itself.

**Attachment name sanitising is written.** It is
[`Attachment::file_name`](../crates/vault-core/src/model.rs): the last component
after `/`, `\` and `:`, without control characters or a leading dot, bounded to
two hundred characters, and `attachment` when nothing is left. The name in the
database is shown as the database holds it and is never used as a path; this is
the name the save panel is offered, and it is the only place the conversion
happens.

**Part of a secret is cut here, and only here.** A reader who selects one
recovery code out of ten and copies it asks for a part of a value, and the
window never holds the value to cut it:
[`SecretValue::part`](../crates/vault-core/src/secret.rs) takes the two positions
the selection reports, counted in UTF-16 code units because that is what a text
node counts in, and hands back that part as a secret of its own. It refuses
rather than rounds: an empty or backwards range, an end past the value, and an
end between the two halves of a character outside the basic plane are none of
them a selection a reader can make, and the refusal says so without a word of
the value. A property test holds it to what a text node would have selected for
the same two numbers.

**Typing a lock finds is written here, on narrower terms than a commit.**
[`Vault::set_typed`](../crates/vault-core/src/vault.rs) is `set_field` - the
entry's previous state kept as a version - for text the reader had typed into a
field and not left when the vault had to lock. Nobody is looking when it runs,
so it refuses what a commit would take: a field the entry no longer has, unless
it is one of the five standard ones every entry is drawn with, is not made
again, because a field of the reader's own removed while its text was on the
way would otherwise come back under their feet. And text that is what the field
already holds writes nothing and marks nothing to save, whatever protection the
draft names, since a field keeps its own; a lock that found only that saves
nothing and says nothing. What was
typed, and in which order it arrived, is the window's business and is kept in
`vault-gui`; this is the one rule for what a draft may do to the database.

**A new value typed in a Change field never goes over a value.** The draft says
how it was typed ([`Typing`](../crates/vault-core/src/vault.rs)): `InPlace` for
a field edited where it stands, `Beside` for a new value typed in a field of its
own under a protected one. The reader never saved a `Beside` value, and it may be
half a password or the wrong one pasted, so a lock that wrote it over the stored
one handed them a password that opens nothing. It goes into the field only when
the field holds nothing - "Set one" on an entry without a password. Otherwise it
goes into a new protected string field of the same entry, named after the one it
was typed for with ` (typed before locking)` after it -
`Password (typed before locking)` - and numbered past a name the entry already
uses by the rule that names a second file (`clash::beside`), and the value it was
typed for stays as it was. Nothing typed, or what the field already holds, is no
new value and writes nothing. `set_typed` answers which of the three happened
([`Written`](../crates/vault-core/src/vault.rs): `Nothing`, `Into`, `Beside`),
so that the unlock screen can say a new value was kept beside the old one
without anything having to name the field. The field is an ordinary KDBX string field of the
kind a reader makes with "+", which every client shows and edits; nothing is
added to the format.

**A field keeps its protection until it is asked to change it.**
[`Vault::set_field`](../crates/vault-core/src/vault.rs) decides protection only
for a field it makes. A field the entry has is written under the protection it
has, whatever [`NewValue`](../crates/vault-core/src/vault.rs) says: the window
says how a field is protected with every value it writes, and what it says is
what it read before the reader pressed anything. With a way to hide a field and
a way to write one travelling side by side, the value written on the way out
of a field could land after the press that hid it and put it back into the file
as plain text. [`Vault::set_protection`](../crates/vault-core/src/vault.rs) is
the one door: it moves a field of the reader's own between the two kinds of
storage inside the vault, with no copy left behind and a version kept, and
writes nothing when the field is already stored that way.
[`Vault::rename_field`](../crates/vault-core/src/vault.rs) moves a value to a
new name the same way, protection and all, in one version, and refuses a name
the entry already uses or one of the five standard names whether or not the
entry has that field yet: a field renamed `Password` would be the password. The
five standard fields can be neither renamed nor hidden nor shown from here:
their names are what every client knows them by, and their protection is the
database's, set for every entry at once. A field renamed or hidden keeps its
earlier state in the version the edit wrote, which is where KeePassXC's
history shows the old name.

**Whether a protected value is in lines is known without revealing it.**
`FieldValue::Protected` carries `lines` beside `empty`: whether the value has a
line feed or a carriage return in it, the way a text area counts a break. It is
one bit about the value and nothing of what it says, and it is what lets the
window replace ten recovery codes in a field written in lines.
`Field::in_lines` answers it for an open value and a protected one alike.

## The recycle bin, and putting things back

`SPEC.md` says a deletion goes to the recycle bin when the database keeps one
and out of the file otherwise. Coffer also takes things back out of the bin, and
says before a deletion which of the two it will be.

**The library keeps where something came from, and hides half of it.**
`EntryMut::move_to` and `GroupMut::move_to` both set `PreviousParentGroup` to
the group being left, and the field is read and written with the rest of the
file (`format/xml_db/entry.rs`, `format/xml_db/group.rs`). The field itself is
`pub(crate)`. The only way to read it is `previous_parent()`, which answers only
when the group it names is still there, so a folder that was never written down
and one that has since gone look the same from here. Both send what is put back
to the top of the vault, which is the right answer for either.

Moving is not an edit. The move uses `move_to` on an `EntryMut` or a `GroupMut`
rather than through `track_changes`, so no version is written, and it records
nothing in `DeletedObjects`: a record there would make every other client delete
the entry at its next merge. Putting back moves the same way, and the folder it
leaves - the bin, or a folder in it - becomes its `PreviousParentGroup`.

**Moving between folders is the same move, closed at the bin.**
`Vault::move_entries` and `Vault::move_group`
([`vault/moves.rs`](../crates/vault-core/src/vault/moves.rs)) are the
deletion's move with checks in front of it: nothing in the bin moves, because
putting back is the way out and says where to; nothing moves into it, because
deleting is the way in and says first whether it can be undone
(`InRecycleBin`, `IntoRecycleBin`); and a batch is checked whole before
anything in it moves, so it moves all or none. A folder the bin sits inside may
move, and the bin goes with it and is still the bin. Making an entry asks the
same question of where it goes (`Vault::destination`), so nothing is made in
the bin either: of a kind, from a template or as a copy (see "Kinds, templates
and copies" below).

The checks stand in front of the library because the library makes none of
them. `EntryMut::move_to` and `GroupMut::move_to` (`db/types/entry.rs:581-599`,
`db/types/group.rs:556-586`) do not ask whether the folder is a new one: a move
to where something already is takes it out of its place, puts it at the end of
the folder, and writes the folder as the one it came from. Coffer leaves such a
thing alone - it is not moved, not answered as moved, and leaves nothing to
save - so a select-all sent to a folder that already holds some of it changes
nothing about those. Neither `move_to` sets `LocationChanged`; only
`EntryTrack::move_to` (`entry.rs:712-717`) does, and that one writes a version.
So Coffer sets it itself, to the second, as `Times::now` gives it.

**A large batch costs the length of the folder, and keeps its order.** Both
`move_to`s take the thing out of its folder with `IndexSet::shift_remove`, which
costs the length of the folder. Emptying a folder of fifty thousand entries in
one batch takes about two seconds - 2.13 s measured on Apple silicon with the
library optimised, as the workspace builds it even in debug - held under the
session's lock the way a save is. Moving from the end of the folder first takes
12 ms and lands everything in reverse. The order is kept instead, because a
batch that size is a select-all and the reader's order is theirs.
`fifty_thousand_entries_move_out_of_one_folder_and_back` is the proof that it
ends; nothing asserts the time.

**Taking a move back goes by what the file says.** `move_entries` answers each
entry it moved with the folder it left (`model::Move`), and
`Vault::move_entries_back` takes that list and the folder they went into. It
moves each back only while the entry is still in that folder and its
`PreviousParentGroup`, which every move writes, is still the folder it left. A
later move writes another, whoever made it, and so does a trip away and back
by way of a third folder; either way the undo is refused whole with
`MoveSuperseded`, and nothing moves. The folder left is what is checked rather
than `LocationChanged`, because a client writing through this library's
`move_to` moves an entry without dating the move. There is no way to put
something at a position, so a move taken back goes to the end of the folder it
came from, and an entry named twice goes once: the second would be the move to
where it already is. An entry gone out of the file since is refused the same
way: that too was done after the move. `Vault::move_group_back` is the same
rule for a folder, which `move_group` answers nothing for because the window
already knows the one folder it moved and where from: the folder must still be
in the folder it went into, with the folder it left as its
`PreviousParentGroup`, and neither of them in the bin. A folder it left that has
since been moved inside it would take it inside itself; the library's own walk
refuses that before anything moves, and it is answered `MoveSuperseded` too.

**Where something goes back to is the folder it came from, while that is
somewhere to go.** Not when it has gone, and not when it is in the bin itself:
an entry whose folder followed it into the bin goes to the top of the vault
rather than into a deleted folder. The top of the vault is a folder like any
other to come from.

**What went in with a folder goes back where the folder came from.** Deleting a
folder moves the folder and nothing inside it, so what is inside keeps the
`PreviousParentGroup` of its own last move, which KeePass and KeePassXC write on
every drag between folders. Read as "deleted from", that sent an entry filed
into Banking a year ago back into the folder it was filed out of. So `Binned`
names the folder it went in with (`within`, the one the bin holds), and `from`
is that folder's own way back: where it would be had the folder been put back
whole, or the top of the vault when that is gone or in the bin too.
`put_back_entries` and `put_back_group` return nothing; where a thing went is
read off the tree like everything else.

**When something went in is the date its folder went in.** A folder takes what
is in it along, and nothing inside it is moved, so its own `LocationChanged` is
the date of some older move. What sits inside a deleted folder reports the
folder's.

**One rule, asked from both sides.** [`bin.rs`](../crates/vault-core/src/bin.rs)
decides where a deletion goes, what the tree says it will do, and what the bin
says about what it holds. A walk of the tree steps down folder by folder with
`Bin::enter`; a single entry is placed by taking the same steps from the top, so
the tree and the entry cannot disagree, and the deletion asks the same function
the window was answered from. The deletion also takes the answer the reader was
shown, and refuses with `DeletionChanged`, changing nothing, when the rule now
gives the other one: a move to the bin never turns into an erasure on the way,
whatever reached the vault first. A folder the bin is inside is erased rather than
binned - it cannot go inside itself - and that is asked of a folder and never of
an entry, because the top of the vault holds the bin and is where every entry
Coffer makes lands.

## Many entries at once

The window acts on several entries the reader chose with one call each, and
offers one undo for all of it. That means something only if a batch is whole.

**Every entry is checked before any is changed.**
[`vault/batch.rs`](../crates/vault-core/src/vault/batch.rs) holds deleting,
putting back, putting a tag on and taking it off again for many entries, and
each looks at every entry it names before it touches one: an entry that is not
there, one whose deletion is no longer the one the reader was shown, one to be
put back that is not in the bin, a tag the format would not give back as
written. One refusal refuses the batch, with nothing changed and `edits` where it
was. An entry named twice is done once. There is no single deletion or put back
beside the batch: the pane's Delete is `Vault::delete_entries` with one entry,
and its Put back `put_back_entries` with one, so each rule is written once.
Moving several entries, and taking the move back, is `vault/moves.rs`'s, on the
same terms.

**An erasure of many is one change to the pool.** `attachment::detach_entries`
is asked once for every entry being erased, and works out where every file goes
before it writes anything, so a version standing in the way of one entry's file
refuses them all with `AttachmentInHistory` and the pool is untouched. It runs
before any move into the bin, because it is the one step left that can say no;
a batch that erased nothing must not have moved the rest of its entries into the
bin either. Two entries naming one file - which KeePass 2 writes for identical
files and Coffer never makes - take it with them when both go in one batch.
Emptying the bin is different on purpose: it erases what can go and says what
stayed.

**Where they land, and what it costs.** A batch into the bin and back goes
through the same `move_to` as a move between folders (see "A large batch costs
the length of the folder" above), so entries arrive in the order they had, each
goes to the end of the folder it lands in, and fifty thousand out of one folder
cost the length of that folder for each: seconds, held under the session's lock
like a save. `batch::fifty_thousand_entries_go_to_the_bin_and_come_back_in_one_batch`
is the proof that it ends. Where each one is going is asked once per folder it
leaves (`bin::Standings`), not once per entry: the walk from the top of the vault
down to that folder is the same for every entry in it. Putting back asks one
more thing of each entry, whether the folder it came from is still there and out
of the bin, which is a walk up from that folder, short and per entry, because
each names its own.

**A tag has one rule wherever it is written.** `text::tag`: text a KDBX file can
hold, not empty, not padded with spaces, no `;`, `,` or tab, all of which the
format reads back as something else. `set_tags` and `tag_entries` both ask it.
A character no KeePass file can hold is refused with `UnwritableText`, as in any
other value; the rest with `UnwritableTag`, whose sentence is about tags, since
the file holds a semicolon or a space anywhere but inside one.
Unlike a move, a tag is an edit: putting one on writes a version on each entry
that lacked it and on no other, and answers those, so the undo -
`untag_entries` with exactly those - leaves the tag on an entry that had it
before. Taking a tag off writes a version only where it was, removes every copy
of it, and asks no spelling of a tag the file already holds; an empty one is
refused, since no window ever put one on. A tag and its undo are two versions,
and a save prunes to `HistoryMaxItems` as it does for any edit.

## Kinds, templates and copies

`SPEC.md` draws one entry: a login. A reader keeps a bank card, the Wi-Fi at
home, a passport and the codes that get them back into an account beside their
logins, and used to build each one field by field. Coffer now makes an entry of
a kind, makes one from the templates a vault keeps, and copies one, all in
[`vault/making.rs`](../crates/vault-core/src/vault/making.rs), and all in the
format's own vocabulary: string fields, a tag and a built-in icon
(`SPEC.md`, section 12). Nothing in the file says which kind an entry was made
as.

**A kind is fields, a tag and an icon.** [`kind.rs`](../crates/vault-core/src/kind.rs)
holds the list. Each kind writes the five fields every entry has, protected as
the database asks, and then its own, empty:

| Kind | Fields beside the five (hidden ones marked) | Tag | IconID |
|---|---|---|---|
| Login | none | none | 0, the key |
| Bank card | Cardholder, Number (hidden), Expires, CVV (hidden), PIN (hidden), Bank phone | `card` | 66, Money |
| Wi-Fi | Network name, Security | `wifi` | 12, IRCommunication |
| Passport / ID | Full name, Number (hidden), Date of birth, Issued, Expires, Issued by | `id` | 9, Identity |
| Software licence | Licensed to, Licence key (hidden, in lines), Order number, Purchased | `licence` | 47, Package |
| Recovery codes | Recovery codes (hidden, in lines) | `recovery` | 52, PaperLocked |
| Secure note | Secret note (hidden, in lines) | `note` | 44, Note |
| SSH key | Public key | `ssh` | 29, TerminalEncrypted |

The network's password is a Wi-Fi entry's own Password, and an SSH key's
passphrase its Password, with the private key a file on the entry
(`SPEC.md`, section 6). A secure note's secret is a hidden field of its own
rather than the entry's Notes, which a database that does not protect notes
sends to the window with the rest of the entry. "In lines" is not in the file:
the format has no such flag, so it is only how the window draws the field
before anything is in it. `kind::SUGGESTED` is the three names the window offers
for a field of the reader's own - PIN, Account number, Security answer - and
whether each is hidden. The icon numbers are KeePass 2's own `PwIcon`
numbering. KeePassXC 2.7.12 names the few icon sources its binary still
carries by the same numbers - `C13_KGPG_Key3`, `C18_Display`,
`C19_Mail_Generic`, `C26_FileSave` for KeePass's MultiKeys, Monitor, EMail and
Disk - which is the evidence the two agree; the pictures behind the eight
numbers above were not seen, and are on the release checklist.

**A login writes its icon now.** `create_entry` used to leave `IconID` out,
which the library writes only for an icon that is set, and KeePassXC drew the
key in its place. A login now writes 0, the same key, so an export reads the
same and the file says it rather than leaving it to the reader.

**What an entry holds is one list.** [`Content`](../crates/vault-core/src/content.rs)
takes an entry's fields, tags, custom data, auto-type, colours, override URL,
quality check, expiry and icon out of it, and puts them onto another. A restore
uses it to put a version back over its entry, and a copy to put an entry onto
a new one, so the two cannot disagree about what an entry is. It is a list
written out by hand, as `history::restore`'s was: `id`, `parent`, `icon`,
`attachments` and `previous_parent_group` are crate-private on the library's
`Entry` (`db/types/entry.rs:52-97`), so an entry cannot be cloned into the tree
whole, and nothing outside the crate can destructure one exhaustively. A field
the library gains is one more line here, and
`restoring_a_version_brings_back_everything_but_the_files` and
`a_copy_holds_every_field_with_its_protection` are where it is missed. The
icon goes through the setters (`set_icon_none`, `set_icon_builtin`,
`set_icon_custom`, `entry.rs:500-541`) because an entry using a custom icon
holds a back-reference in it; a custom icon that has gone becomes none.
Unlike files, custom-icon back-references are rebuilt on load
(`format/xml_db/mod.rs:161-193`), so a copy naming one is as safe as the
original. One `put` serves both a tracked edit and a new entry:
`EntryTrack::as_mut` (`entry.rs:704`) is public and hands back the `EntryMut`
the restore writes through.

**A copy is a new entry.** `Vault::duplicate_entry` puts the copy in the
original's folder, at its end, called what it is called with " copy" after it,
and `Vault::create_from_template` puts one in the folder asked for under the
template's own title. A new id from `GroupMut::add_entry`, an empty history
(`Entry::with_id` starts one), and every date the moment it was made
(`Times::new`) but the expiry, which the reader chose and the copy carries; an
entry with no expiry date at all gets the one everything Coffer makes gets
(`dated`). The original writes no version. A title the database protects stays
protected, and " copy" is put on it inside the box it is kept in. The new name
is built at its full length at once rather than grown: a string that grows moves
to a larger allocation and leaves what it held so far behind, unwiped, and here
that would be the protected title. An empty
title stays empty: "Untitled" is the window's word for none, and a copy called
"Untitled copy" would put it into the file. An entry with no title field gets
none. Copies are not numbered: a title is not a key, and the window opens the
copy with its name selected to be typed over.

**A copy's files are its own.** [`attachment::copy`](../crates/vault-core/src/attachment.rs)
puts a copy of every file the entry carries on the new entry through the
module's own `put`, each a new member of the pool at its end - on an unbroken
pool `AttachmentId::next_free` is its length. Never a second name for the same
file: a file two entries name is the one shape the pool can only refuse to move
(`Refusal::Shared`), so a copy sharing the original's files would pin both for
good, and neither could lose a file without the other. The bytes are cloned in
memory, so copying an entry carrying a hundred megabytes holds the session for
that long and the vault grows by that much; a copy that would take the file past
the ceiling is refused at the save like any other (`TooLarge`). A source that
gives one file two names - the fixture's "attachments" entry does - gives the
copy two files.

**The templates group is the one the file names, while something can be made
from it.** `Meta/EntryTemplatesGroup` names a group KeePass 2 and MacPass offer
templates from; the library reads and writes it and does nothing else with it.
[`templates.rs`](../crates/vault-core/src/templates.rs) says which group counts:
the one it names, when that group is there, is not the top of the vault, and is
not the recycle bin or inside it. A nil UUID names none, as KeePass writes it, even
in a file where some group carries that UUID.
`Project::is_templates` marks that group in the tree, and
`create_from_template` refuses an entry the group does not hold itself with
`NotATemplate`: an entry in a folder inside it is not one (KeePass 2 draws such
a folder as a list of its own, which Coffer does not), and neither is one whose
group went to the bin since the window drew its list. The field is cleared when
the group goes out of the file, as before (`forget_groups`); Coffer never sets
it.

## The pool of files, and why removing one is not a removal

Every attachment in a KDBX 4 file lives once, in the inner header, and an entry
refers to one by a number. The reader hands those numbers out **by position in
the header**; the writer orders the header **by the number an attachment already
carries** and writes the number itself into the entry. The two agree only while
the numbers are an unbroken run from zero. Punch a hole and the file still
saves, still opens, and hands the bytes of one attachment to the entry that
asked for another - the right file name over somebody else's file. That is
worse than losing it.

Nothing in the library keeps that run unbroken. Three things make it Coffer's
job:

- **`Attachment::entries`, the reference count, is empty on every database read
  from disk.** The only line in the crate that fills it is
  `EntryMut::add_attachment`. So `remove_attachment_by_name`,
  `remove_attachment_by_id`, `AttachmentMut::remove` and `EntryMut::remove` all
  read "nothing points at this" and destroy a file two entries share.
- **`AttachmentId::next_free` fills holes rather than appending.** A removal
  followed by an add repairs the damage by accident, which is why the corruption
  is intermittent and why the regression test removes and saves with nothing in
  between.
- **`AttachmentId::new` is crate-private and previous versions cannot be
  rewritten**, so a number can never be changed: not on a version, and not
  anywhere else.

[`attachment.rs`](../crates/vault-core/src/attachment.rs) is the whole answer.
It works out what names each file, current entries and previous versions alike;
lifts a file out of the pool before taking a name off it, so the library's own
removal finds nothing to destroy; and puts back what survives, lowest slot
first, so the run closes up with nothing moved that anything unrewritable points
at. Two things follow that a reader of the source would otherwise find
surprising:

- **A file a previous version still holds is not removed.** There is no way to
  keep bytes in the pool without a name on some current entry, and no way to
  take the name off the version. Coffer says how many versions hold it, and the
  entry screen offers the only thing that lets it go: clearing those versions.
- **The files on an entry are not part of its history.** Neither adding one nor
  taking one away writes a version, and a restore does not touch them. A version
  written across a change to the pool would name bytes that have moved or gone,
  and a version written on the way *in* would be the surest way to make the file
  impossible to take off again. What a version records, and what a restore
  puts back, is the list in [`content.rs`](../crates/vault-core/src/content.rs):
  the entry's fields, tags, custom data, auto-type, colours, override URL,
  quality check, expiry date and icon.

**A file never goes on an entry over one of the same name.** An entry keys its
files by name, and the library's `EntryMut::add_attachment` drops whatever the
name held before - which is how the second page of a passport, scanned by a
phone that calls every scan `Scanned Document.pdf`, used to take the first with
it and leave no version to find it in. `Vault::add_attachment` now changes
nothing when the name is taken and answers `Attached::Taken(Clash)`: the size of
the file there and the name the new one could go by. The caller decides, through
`Vault::keep_both`, which puts it beside the old one under that name worked out
again at that moment, or `Vault::replace_attachment`, which is a removal by this
module's rules followed by an add, and is refused exactly when a removal would
be. Both take the bytes by reference, so a caller told the name is taken still
has them to answer with.

The free name is [`clash.rs`](../crates/vault-core/src/clash.rs): a number before
the last extension, or at the end of a name that has none - a dotfile, a
trailing dot, and a dot followed by a space are not one - counting from 2 past
every name on the entry that matches it with letter case ignored. Whether a name
is taken at all is exact, because the format and the library both tell
`Scan.pdf` from `scan.pdf` and adding the second loses nothing. Only the files
the entry has now count: a name only an earlier version gives a file is free,
and the version keeps its bytes.

`unbroken` runs before every write. It has never fired, and it is the last thing
between a mistake in this module and a database that hands out the wrong file.

One shape only another client can produce is worth naming: KeePass 2 pools
identical binaries, so **one file can carry two names**, and `rich-kdbx41.kdbx`
has one. Taking one of the two names away keeps the file exactly where it is,
which is tested. Making such a file *move* would mean rewriting both names and
only one can be rewritten, so that is refused - a guard against a database
Coffer cannot itself produce, and the reason the refusal has no test of its own.

## Memory, and what a lock leaves behind

Two things, and they cover different halves of the same question.

[`wipe.rs`](../crates/vault-core/src/wipe.rs) empties the tree Coffer still
owns, at the moment it stops owning it. Every write goes through `Zeroize`,
which is a volatile store per byte followed by a barrier and covers a buffer's
whole capacity: a plain `fill(0)` is a store into memory that is never read
again, and the optimiser may remove it. Wherever the library allows it a value
is written over where it lies rather than dropped, because a buffer that is
still this process's can be read back and asserted to be zero. The maps are the
exception - a map hands its keys out by shared reference, and the name of a
custom field is the user's text as much as its value is - so those are drained.
Two things cannot be reached at all: an entry's attachment *names*, and the
unprotected halves of previous versions, both of which the library keeps
private.

The wrapper that carries the destructor is on the `Database` field rather than
on `Vault`. Reloading assigns into that field, and reading the file on disk to
answer the conflict dialog decrypts a whole second database that no vault ever
owns; a destructor on `Vault` would see neither.

[`scrub.rs`](../crates/vault-core/src/scrub.rs) covers what has already been
handed back. Opening a KDBX file decompresses the whole database into a buffer,
deserialises it into a second, decrypts every protected value into a third and
frees all three; none of them belong to Coffer and the library zeroizes none of
them. A database of six hundred entries opened and dropped under the system
allocator leaves over two thousand readable copies of a password in freed heap.
Under the allocator in `scrub.rs` the same measurement is zero, at about six per
cent of a parse and nothing measurable on a save.

It is declared in this crate rather than in the window's so that it covers the
test binaries: the suite that asserts no value survives a lock has to be running
under the thing that makes that true. macOS writes over a freed block of its own
accord up to about sixteen kilobytes, which is why `tests/vault/wipe.rs` uses a
crowded database - a small one measures clean whatever Coffer does. Linux zeroes
nothing at any size.

Neither reaches an Objective-C allocation or another process. The pasteboard is
both, which is why a copied secret is taken back on a timer rather than trusted
to a wipe.

## The copy a lock leaves, and putting it back

[`storage/unsaved.rs`](../crates/vault-core/src/storage/unsaved.rs) is the whole
life of the copy `Vault::rescue` writes: where it goes, whether one is there,
removing it, and the two ways it becomes the vault again. Which vault a copy
belongs to is read off its name by `unsaved::taken_from` and recorded nowhere
else - not inside it, where it would be a field no other client knows, and not
in a file beside it that could disagree with the name.

**Putting a copy back is a staged write published at a name nothing holds,
not a rename.** `rename` replaces whatever is at the target, and the whole point
of `unsaved::put_back` is that it never does: it runs only for a vault whose
file has gone, and without a password. So the copy's bytes go through
`atomic::stage` into Coffer's own temporary file, and `Staged::publish` gives
that file the vault's name with `hard_link`, which fails with `AlreadyExists` if
anything is there, a dangling link included. Nothing is at the vault's name
until it is whole, a file that arrived first is `DatabaseExists` and is neither
written over nor removed, a copy that is not there leaves nothing at the name,
and a process killed part way leaves the name empty and the copy where it was.
The copy is removed only after the publish. A filesystem without hard links is
`NoExclusiveMove`: an exclusive create followed by a copy would be a name
holding half a database while it ran, so the copy is opened and promoted
instead. The lock files beside both names are taken for as long as it runs,
with `lock::claim`, which is stricter than opening: a lock somebody else
holds is a refusal, not an offer to take it over.

**Making a copy the vault is an ordinary save aimed at another name.**
`Vault::promote` points the vault at the name `taken_from` gives, takes the lock
beside it, and runs the same write `save_over` runs, so the place is proved
writable, what is there becomes `<vault>.1.bak`, and the stamp is recorded as
for any save. It takes the `storage::Seen` of the vault's file the reader was
shown - its time and its length, inode and device - and with the lock held
refuses with `VaultFileChanged` when the file stands otherwise now, or when
nothing was shown: `Guard::Ignore` would otherwise push a change nobody saw into
the snapshots. If the write is refused or fails, the path and the stamp are put
back and the vault is still the copy; only once it went through does the vault
keep the new lock, drop the copy's, and remove the copy.

**A copy's snapshots go with it.** Saves inside an open copy rotate a chain
beside the copy's name. `unsaved::retire` clears those slots once the copy is
promoted, put back or discarded: nothing lists them after that, and they would
be old states of the vault opening with an old password.

**Whether a file is there is its own question.** `storage::on_disk` answers it
for the screen - gone only when nothing at all is at the name, and a link that
leads nowhere is still something - and it is only ever advice. The two moves
above ask the disk again, with the publish's hard link and with `Seen` under the
vault's lock.

## Backups, and making one the vault

The ten snapshots are what the screen calls backups. Everything here is about
reading the right one back, and about putting one in the vault's place without
losing the file it replaces.

**A snapshot is found by what it is, not by where it sits.** `snapshot::Taken`
carries the `Seen` of the slot's own entry: inode, device, length and time.
`rotate` renames, which keeps all four, so `snapshot::find` answers where a
snapshot listed at one slot is now. Slot 1 is a hard link to the database it
replaced, and shares that file's identity only until the commit gives the
database a new inode; no two slots share one. On a filesystem that numbers a
file anew when it is renamed (FAT and exFAT may), a rotation between the list
and the press finds nothing, which is a refusal and never another snapshot.

**Which vault a snapshot belongs to is read off its name.** `snapshot::taken_from`
takes the slot off the end, as `unsaved::taken_from` takes the suffix off a
copy, and answers nothing for a name that is not a snapshot's. Nothing inside a
snapshot or beside it records whose it is.

**Read only has four reasons, and one kind of them is about the data.**
`Vault::read_only` answers `ReadOnly::Snapshot`, `Place`, `Kdb` or
`Kdbx3Attachments`, decided once by `classify` from the format and the place and
held where the vault keeps it; `writable` refuses with the matching error.
`Vault::copyable` is whether `encrypt_copy` would take what is open somewhere
else: a snapshot and a place are about where the file is, and a copy goes
somewhere else, while the two formats are why the bytes cannot be trusted, and a
copy would carry them. `encrypt_copy` asks the same question, so the screen's
offer and the write never disagree. `Vault::files_readable` is the same for a
file's bytes: everything but `Kdbx3Attachments`, whose names lead to another
entry's bytes (see above). `attachment` refuses on it, and a menu under the
pointer greys its Save to… on it, so those two never disagree either.

**Making a backup the vault is the ordinary write aimed at the vault's name.**
`Vault::adopt` and `Vault::promote` share `take_over`: the vault is pointed at the
vault's name with the lock beside it held, judged for that name before the write
(a snapshot's own name refuses every write), written the way `save_over` writes,
and put back whole - path, stamp, content and what it was judged - if the write
does not go through. `adopt` refuses anything but a snapshot with
`NotASnapshot`, and a file at a snapshot's name in a format Coffer does not
write for its format. A snapshot of the copy a lock left is refused with
`NotASnapshot` as well: the copy is what it would go over, the copy holds the
only version of work its vault has not got, and nothing but the copy's own
saves writes over it - `promote` would retire the snapshot that kept what it
held a moment later. It holds the press to the `Seen` the reader was shown,
asked once the vault's lock is held, as `promote` does. The key is the one the
backup opened with, so no password is asked for, and the vault opens with that
key's password from then on - after a change of the master password, the old
one. A write that failed after `rotate` moved the open backup a slot on has the
vault follow it there by `find` (`settle_failed_adoption`, whose test makes the
rotation by hand, since no fault from outside lands between it and the rename);
the oldest backup, which the write's own rotate removes, stays open from memory
under the name it had, and a write that did go through wrote it whole into the
vault before that mattered.

**A vault file that will not open with the backup's key is kept by a second name
first.** One that opens is an older or newer state of the same vault, and the
chain keeps it as the newest snapshot like anything a save replaces. One that
does not - damaged, under another password, not a vault - would be in the chain
only until ten later saves pushed it out, and nothing in Coffer could open it to
say what it held. So `storage::aside::keep` gives it
`<vault>.replaced-YYYY-MM-DD.kdbx` before the write, numbered from `-2` past any
name already taken, and nothing removes it. Asking costs a key derivation, so it
is asked once, at the press, and only of a file that is there.

The second name is a hard link, which refuses a name that is taken - a link
that leads nowhere included - and shares the file rather than copying it. Its
mode loses what anybody but the owner may do and gains nothing, because the
link is the vault's own file: made owner-only outright, a vault its owner had
made read only would come back writable and the write that follows would go
over it, where a save checks first and refuses. On a disk without links the
name is taken with an exclusive create and filled through a staged write, so it
holds Coffer's empty file or the whole of the vault's, owner-only from birth.
The `.kdbx` on the end is load-bearing for the reason the copy a lock leaves
ends in it: every file panel filters on it, and a file under another password
has to be pickable and openable with that one. The name is none that
`storage::reserved`, `unsaved::taken_from` or `snapshot::taken_from` reads as a
file of the vault's own of another kind, so nothing offers it as a copy, rotates
it or retires it; `home::found` treats it as the vault of its own it is.

Why a second name rather than moving the file away and publishing the backup
exclusively at the empty name: a move followed by a publish leaves a moment with
nothing at the vault's name, and any failure between the two - a disk without
links, a file that arrives - leaves no vault at its name at all, where this
order never does. It is also the standard library alone: an exclusive rename is
`renamex_np(RENAME_EXCL)` on macOS, a capability each volume may or may not
have, and `renameat2` elsewhere. The file still lands in `.1.bak` through the
ordinary rotate as well, so the guarantee is every save's, plus a name no save
touches. A press that does not go through takes the second name back with
`aside::withdraw`, and only while the vault's name still holds the very file -
the same inode, or the same bytes in a copy - so a refused press leaves no `-2`,
`-3`... behind it, and a file that may have no other name left keeps this one. A
process killed between the second name and the commit leaves the vault whole
and one extra name for it. A refusal of the second name is `VaultFileChanged`
only when the vault's file no longer stands as the reader was told, asked again
then; any other is the disk's own answer, so a file that never changed is never
refused as one that did.

A symbolic link somebody left at the vault's name is linked as the link, both
for the second name and for slot 1, and neither is tightened: a link's own mode
means nothing, and tightening through it would change a file outside anything
Coffer keeps. The write then puts the backup at the vault's name as a file of
its own. A link that leads nowhere - a vault on a disk that is not plugged in -
is kept the same way, and the chain does not move, since nothing is there to
snapshot.

## A copy somewhere else

**A copy is encrypted under the vault and written without it.**
[`Vault::encrypt_copy`](../crates/vault-core/src/vault/copy.rs) settles every
history the way a save does, derives the key and encrypts the whole database
into memory, refusing a copy past the ceiling as a save would; then
`EncryptedCopy::write` puts those bytes at the name. The first needs the tree
and is done by whoever holds the vault. The second needs nothing of it, and it
is aimed at a disk that is not the vault's - a stick pulled out half way, a
share whose server went to sleep - so vault-gui writes it with the session let
go, and a lock, a sleep or Quit never waits on that disk. A lock that lands
between the two takes the vault and leaves the bytes, which are the vault under
its own credentials as it was when they were encrypted
(`elsewhere::a_copy_encrypted_before_the_vault_closed_is_written_whole_after`).

**A copy never replaces a file.** It takes no snapshot of what is at the
name it is given, and the panel it is aimed from may open in the vault's own
folder, where the vault a backup was taken of, the copy a lock left and a file
kept aside all end in `.kdbx`. So the write refuses a name that holds anything,
a link that leads nowhere included, with `DatabaseExists` - a name taken while
the copy was being encrypted as well - and `encrypt_copy` refuses a name
`storage::reserved` reads as a snapshot's or a lock's copy's with
`ReservedName`, as `create` does. The copy is staged beside the name first, the
name is taken with an exclusive create only once the copy is whole, and the
rename puts the copy over that empty reservation; a rename that fails takes the
reservation back. That works on a disk without links as well, where a publish
by hard link would not, and the name never holds part of a copy. The lock's
rescue is the one write of a copy that goes over a file: it writes the same
name every time, the copy an earlier lock with the same trouble left, through
the same staged write and the same encryption.

**A copy aimed at a link to the vault leaves the vault whole.** A save panel
asks before it replaces a file, and a reader who points it at a link to their
vault and agrees is refused before anything is written: a symbolic link is
followed to the vault and refused as `CopyOntoItself`, and a second name for the
vault's file is a name that holds a file, `DatabaseExists`. The vault's bytes,
its inode, the link and the backups are as they were
(`elsewhere::a_copy_aimed_at_a_link_to_the_vault_leaves_the_vault_whole`). A
copy settles every history as a save does, carries a change the vault has not
saved without saving it, and leaves the vault's file, its lock and its backups
untouched however many are made.

**When a vault was made is what its top group says.**
[`Vault::made`](../crates/vault-core/src/vault/began.rs) is the top group's
creation time, which KeePass, KeePassXC and Coffer write when they make a
database - the library dates every group it builds - and which no client moves
later; a save and a copy keep it. It is `None` when the file has none, and handed
back whatever year it is. The window counts how long a vault has gone without a
copy on another disk from it when none was ever made, and decides there what
1600 and 3000 mean. It is read and never written: nothing Coffer keeps about
copies goes into the file.

## Making a vault, and what a second of work costs

[`kdf.rs`](../crates/vault-core/src/kdf.rs) measures rather than assumes. Coffer
ships for two architectures, and an iteration count tuned on Apple silicon is
five seconds of waiting on an older Intel machine. Argon2id, sixty-four
megabytes and four lanes are fixed by the spec; only the number of passes is
measured, and it is measured by timing a whole save of an empty database rather
than by calling Argon2 directly. The library builds a nine-field configuration
around every derivation - the variant, the version, the lane count, and the
bytes-to-kibibytes conversion the format needs - and restating that here would
be a second copy of a rule that drifts silently in either direction. Leaving
`thread_mode` at its default alone measures four times slow and writes a header
four times weaker than intended.

The search discards its first derivation, probes from four passes rather than
one, takes the lowest of two runs at every step, and verifies up to three times
against the spec's own band. The first derivation in a process costs about half
as much again as the ones after it; a measurement at one pass is a fifth high,
because the arena is allocated and the prehash computed whether there is one
pass or a hundred; and interference makes a derivation slower and never faster,
so the lowest reading is the one closest to the truth and erring that way asks
for more passes rather than fewer. Measured here: 122 passes for a second,
reached in 2.4 seconds of measuring.

Nothing on that path touches the reader's password. `rust-argon2` zeroizes
nothing, so a calibration made with the real one would leave a handful of copies
of derived material in freed heap; it derives from a constant instead.

**The ceiling lives with the check that reads a file.** `preflight::acceptable`
is called by the pre-flight every file passes on its way in *and* by the
calibration's answer on its way out, so a database Coffer writes cannot be one
Coffer then refuses to open. At sixty-four megabytes the product ceiling caps
the count at exactly 1024, long before the count ceiling of 100,000 does.

**The name is taken rather than asked about.** A staged write renames over
whatever is at the target and a creation rotates no snapshot, so a creation
aimed at somebody's vault is the one way this application could destroy one -
and asking whether a file is there and writing one afterwards leaves the whole
of key derivation, a calibrated second, between the question and the answer. Two
Coffers at one name both passed that question and the second rename destroyed
the first one's vault. `atomic::reserve` makes the question and the answer the
same act: the target is created empty and exclusively, and the caller owns it
from then on and takes it back off the disk however the creation ends. The lock
file beside it is taken before any contents are written, so a creation that
cannot have the database never wrote one. `atomic::taken`, inside the crate,
asks the same question without taking the name, for putting a copy back, which
wants to refuse before it reads a large one; a unit test holds it to what the
reservation and the publish refuse. The creation screen asks `vault-gui`'s own
reading of the place, `home::standing`, which also says what is there, and a
test there holds it to what `Vault::create` takes. Either answer is advice, and
the reservation is still what decides.

A name Coffer gives a file of its own beside a database is refused outright: a
snapshot's would open like any other database and then refuse every save, for
good, and a rescue copy's would be written over by the next lock that had
something to keep. `storage::reserved` is the one rule for both, and the window
asks it too, so that neither is ever remembered or offered as the vault. A master
password with nothing in it is refused outright as well, and so is a change to
one.

So is a name with a rescue copy beside it, as `CopyBeside`. The copy is what is
left of a vault whose file went while it was open, and it is put back from that
name's unlock screen. A vault made at the name would be offered the copy as its
own unsaved work, under a password that does not open it, and its next lock with
something to keep would write over the only copy there is.

**A new database is written down in full.** Every `Meta` field is skipped when
it has no value, so a bare new database writes a `<Meta>` carrying a generator
and almost nothing else, and other clients fill the gaps with their own defaults
on load. That is invisible to the round-trip suite, which compares two exports
and therefore compares two substitutions. The same reasoning covers the expiry
date the library omits when nothing expires: the format allows it and no other
client does, so a reader that finds none puts its own there, and two readings of
the same file disagree. Everything Coffer makes carries its own dates.

## Changing the master password

**The key changes in place, and only the password in it.**
`Vault::change_master_password` replaces the password inside the `MasterKey` the
vault holds and nothing else. A key file the vault was opened with stays part of
the key. A vault whose owner opened it with a key file alone gets the password
beside that key file: before, there was no password in the composite at all,
the second composition `unlock` tries. Taking a password or a key file away is
not offered. A key with no password is refused as a creation's is, and key files
are read and never made (`SPEC.md`, section 3). A new password that is not text,
or that is the one the vault already has, is refused as well: the first could
never be typed again, and the second would rotate a snapshot and change nothing.

**A lock's copy is given no new password until it is the vault.** The copy a
lock left opens with `read_only()` answering nothing - its name is not a slot's
and its lock is taken - so `writable` alone would let a change through. It would be the copy's
alone: the vault and every snapshot of it would go on opening with the old
password, the count would be the copy's own chain, and after `promote` the
vault's chain - every slot of it under the old key - would be remembered
nowhere, since `retire` has taken the copy's chain away. So
`change_master_password` refuses a path `unsaved::taken_from` names, as
`PasswordOfACopy`, before the current password is asked about. Made the vault,
the copy has the vault's path and the change counts the vault's whole chain.

**Nor is a vault with such a copy beside it.** The copy opens with the password
the vault has now, and every sentence the unlock screen says about it - the card
about work that never reached the vault, the banner inside the copy, the copy of
a vault whose file has gone - says it opens with the same password. A change
beside it would make each of those false, and "Make this my vault" or putting
the copy back would then write the copy, under its own key, over a vault the
reader had just given a new password: the old password back, with nothing on the
screen to say so. So while `unsaved::found` finds a copy beside the vault, a
change is refused as `PasswordBesideACopy`, before the current password is asked
about, until the copy is made the vault or removed. A copy a lock writes after a
change is under the new key, like everything else written then, and the
sentences stay true.

**The current password is asked of the key, not of the file.** Both are hashed
with SHA-256 and the digests compared with `constant_time_eq_32`, so the time
taken says nothing about how close a guess came, whatever its length: the
crate's own comparison is constant only between two values of one length. A
SHA-256 of the password is itself one of the elements KeePass derives the key
from (`key/mod.rs:203`), so both digests are `Zeroizing` and gone when the
comparison returns. The library builds the same digest into a plain `Vec` every
time it derives a key, which only the allocator in `scrub` wipes; the wipe suite
looks for both digests, as well as both passwords, once a vault given two new
passwords has gone. `constant_time_eq` is the crate `rust-argon2` already brings
in for the same job; naming it adds an edge to the graph and no crate. The check
costs no key derivation, which makes it a far cheaper test of a guess than an
unlock; the window's session makes a wrong one cost what an unlock does
(`docs/ipc.md`, "The current password is checked against the key the vault
holds").

**The write is an ordinary save.** `dump_kdbx4` writes `db.config.kdf_config`
as it stands and draws a fresh master seed, outer IV, inner stream key and key
derivation salt on every save (`format/kdbx4/dump.rs`,
`KdfConfig::get_kdf_and_seed` in `config.rs`). So a change keeps the database's
own key derivation - Argon2id at its calibrated cost, or whatever another client
chose - and is as fresh as any save. `Meta/MasterKeyChanged` is set to now, and
it is the one field the round trip sees move.

**A write that does not go through puts the old key back.** The old password is
held, as `key::Former`, until the write answers. It goes back into the key, with
the old `MasterKeyChanged`, when the write's guard refused it before anything
was written (`ExternalChange`, `DatabaseGone`), or when the file at the vault's
name is still the one the vault last agreed with. The guard's refusal goes
straight back, and is never asked whether the file opens with the new key: a
file another client wrote is somebody else's whatever key opens it, and one that
gave the vault the very password the reader then asks Coffer for opens with it.
Taken for the change's own write, its stamp would be recorded, no conflict would
be raised, and the next save would go over the other client's work. A test
writes exactly that file - another client's, under the new password, with other
content - and asks that the conflict stands, the next save is still refused, and
the file is theirs byte for byte.

The one subtle failure comes after the guard has let the write through and the
rename has happened: a directory that will not flush, or a file that cannot be
read back. The file at the name is then the new one, under the new key. Putting
the old key back would encrypt the next save under a password the reader was
told did not take, while the file opened only with the one they were told it
did. So a failed write that has moved the file is opened with the new key
(`Vault::written_anyway`, decided in `Vault::settle_failed_change`). Past the
guard the file was the vault's own a moment before, so a file that opens with
the new key now is this write's: the change stands and the vault records the
file as the one it agrees with, so the next save is not refused as somebody
else's write. A file that does not open is somebody else's, and the old key goes
back for the conflict that follows. Another client that wrote the file under the
very same new password in the moment between the guard and the failure would be
taken for this write; telling the two apart there would mean carrying the digest
of the bytes this write staged out of a failed write, through the path every
save takes. No fault from outside lands in that window, so both arms of
`settle_failed_change` are covered by unit tests in `vault/rekey.rs` that leave
such a file by hand, and that save afterwards.

**Snapshots are old by what they are, not where they are.**
`snapshot::Superseded` records every snapshot beside the vault as the change
leaves them, by `snapshot::Taken::seen`: inode, device, length and modification
time. Every later save renames each one a slot down, which keeps all four, and
puts a snapshot written under the new key in slot 1, which is another file.
`Vault::remove_old_snapshots` removes the slots that hold one of those files,
wherever they are now, and only slot paths. A second change records the whole
chain again, because all of it then predates the newest key. The time and the
length are in the identity because a filesystem that reuses inode numbers (ext4
does) could otherwise hand a removed snapshot's number to a new one. A slot is
known by the name's own entry, not by what it leads to: Coffer never puts a link
at a slot, and one somebody else put there may lead to the vault itself, whose
identity every later save hands to the snapshot it pushes into slot 1. Such a
link is counted as the link and removed as a link.

**One slot nobody can read hides nothing.** `snapshot::taken` asks about each
slot on its own and passes over one the filesystem will not answer about - a
link that leads round in a circle, a disk that answers with an error - the way
it passes over a hole. The count after a change and the removal both cover every
slot that can be reached. A listing that failed whole for one such slot would
have the change answer that nothing is left under the old password, and every
removal fail for good. A removal that could not take every file answers with
`snapshot::Removal` rather than an error: how many went, how many are left and
remembered for the next press, and why the first of those would not go.

**What a change cannot reach.** Every copy the reader made elsewhere goes on
opening with the password it was written under. The copy a lock left beside the
vault (`<vault>.unsaved.kdbx`) is the one copy of work the vault has not got and
is never removed on Coffer's initiative, which is why a change waits for the
reader to make it the vault or remove it rather than leave it behind under the
old password.
