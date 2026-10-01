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
private and `add_entry` is the only way in.

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
which is what locking has to mean.

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
perfectly well. It opens, `is_read_only()` is true, every change answers
`ReadOnlyPlace`, and `save_copy` to somewhere writable is the way off the
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
  impossible to take off again. What a version records is the entry's fields,
  tags, notes, colours, icon and expiry date.

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
cannot have the database never wrote one. `atomic::taken` asks the same question
without taking the name, for a screen that wants to say a place is taken before
anybody types a password; its answer is advice, and the reservation is still what
decides.

A name Coffer gives a file of its own beside a database is refused outright: a
snapshot's would open like any other database and then refuse every save, for
good, and a rescue copy's would be written over by the next lock that had
something to keep. `storage::reserved` is the one rule for both, and the window
asks it too, so that neither is ever remembered or offered as the vault. A master
password with nothing in it is refused outright as well.

**A new database is written down in full.** Every `Meta` field is skipped when
it has no value, so a bare new database writes a `<Meta>` carrying a generator
and almost nothing else, and other clients fill the gaps with their own defaults
on load. That is invisible to the round-trip suite, which compares two exports
and therefore compares two substitutions. The same reasoning covers the expiry
date the library omits when nothing expires: the format allows it and no other
client does, so a reader that finds none puts its own there, and two readings of
the same file disagree. Everything Coffer makes carries its own dates.
