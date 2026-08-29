# The vault engine

`vault-core` opens a KDBX file, hands back the tree, hands back one value at a
time, applies a change, and saves with a snapshot behind it. It knows nothing
about Tauri, a window or a webview.

Everything below is what a reader of the source cannot work out from the source:
what the library underneath does, where Coffer works around it, and which
hazards are waiting for the slices that come next.

---

## The library underneath

Coffer is built on `keepass`, pinned to exactly `=0.13.22`. The pin is not
caution: `0.13.x` has shipped breaking refactors inside patch releases. Bump it
by hand and run the round-trip suite.

Only the `save_kdbx4` feature is enabled. `totp` is out of scope, and
`challenge_response` pulls in USB access, does not compile against the current
`nusb`, and buys nothing: reading a hardware-key database needs the hardware.

### What the library gets wrong, and what Coffer does about it

Line references are into
`~/.cargo/registry/src/index.crates.io-*/keepass-0.13.22/`.

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

**`VaultError::ReadOnlyKdb` has no test.** No available tool writes a KeePass 1
file: KeePassXC 2.7 dropped the writer and the CLI never had it. The guard stays
because saving a KDB-sourced database as KDBX 4.1 is exactly the kind of silent
conversion the first hard rule exists to prevent.

---

## What slice 2 must take from here rather than invent

Two rules belong in `vault-core` and are not written yet, because writing them
now would be a public function with no caller:

- **The URL scheme allowlist.** `SPEC.md` calls a `javascript:` URL in the URL
  field "the one real code execution path in this application". `http`, `https`,
  `mailto` and `ftp` are the allowed schemes. `vault-core` hands URL values back
  exactly as the file holds them, leading whitespace and mixed case included -
  `open::dangerous_url_schemes_are_handed_back_unchanged` pins that - so the
  check has to normalise before it compares. Put it in `vault-core` with a table
  test, and call it from the frontend.
- **Attachment name sanitising.** Attachment names come from the file and may
  contain anything: the fixture carries `../../escape.txt` and
  `nested/path/name.txt`. `vault-core` returns them verbatim. Whatever turns one
  into a path for the export dialog belongs in `vault-core` too, called from
  there and from nowhere else.
