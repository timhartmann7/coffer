# Golden fixtures

Databases the round-trip suite reads, writes and compares against KeePassXC. Every
one of them is produced by `keepassxc-cli`, never by Coffer, so that the suite
measures us against an external implementation.

Regenerate with `./generate.sh` after changing anything in `sources/`, and commit
the resulting `.kdbx` files.

| Fixture | Format | Password | What it covers |
|---|---|---|---|
| `rich-kdbx41.kdbx` | KDBX 4.1 | `coffer-test` | the whole feature matrix |
| `rich-kdbx40.kdbx` | KDBX 4.0 | `coffer-test` | the same content without the 4.1-only elements |
| `rich-kdbx31.kdbx` | KDBX 3.1 | `coffer-test` | the same content again, attachments in `Meta/Binaries` |
| `legacy-empty-attachment-kdbx31.kdbx` | KDBX 3.1 | `coffer-test` | a zero-byte attachment in `Meta/Binaries` |
| `minimal-kdbx41.kdbx` | KDBX 4.1 | `coffer-test` | one group, one entry |
| `whitespace-kdbx41.kdbx` | KDBX 4.1 | `coffer-test` | values made only of whitespace |
| `keyfile-kdbx41.kdbx` | KDBX 4.1 | `coffer-keyfile` + `keyfile.key` | a database that needs a key file |
| `empty-kdbx31.kdbx` | KDBX 3.1 | `coffer-test` | `keepassxc-cli db-create` output, untouched |

`rich.xml` covers: nested groups five deep, group notes, `IsExpanded`,
`DefaultAutoTypeSequence`, `EnableAutoType`, `EnableSearching`, group tags,
custom icons with and without a name, `CustomData` on the database, a group and
an entry, entry colours, `OverrideURL`, entry tags, `QualityCheck`,
`PreviousParentGroup`, `AutoType` with an association, thirty custom fields half
of them protected, empty protected and unprotected values, a field key with
spaces and an ampersand, Unicode with combining marks and a right-to-left
override, XML entities and a `]]>` sequence, `javascript:`, `data:`, `file:` and
`vbscript:` URLs, attachments holding all 256 byte values, zero bytes, an SSH
private key and a `KeeAgent.settings` blob, attachment names containing `..` and
`/`, six history versions on one entry, timestamps in 1600 and 3000, an entry
with every field empty, a recycle bin holding a deleted entry, and two
`DeletedObjects`.

## Fixture provenance

Produced with KeePassXC 2.7.12. `keepassxc-cli` chooses the KDBX version from the
features a database uses and offers no way to ask for one, so the three format
variants are made by stripping the elements that force a version up; the rules
are written down at the top of `generate.sh`. Fixtures from further KeePassXC
versions are added by installing that version and rerunning the script: the
filenames carry the format, not the producing version, so a second set lands
beside this one rather than overwriting it.

## What the round-trip suite normalises before it diffs

`keepassxc-cli export -q -f xml` output is not byte-stable, and the two sides of
the comparison are not always in the same format. These differences are
normalised away; anything else is a failure.

- **Timestamps.** KDBX 3 exports them as ISO 8601, KDBX 4 as base64 seconds since
  year 1. Both are canonicalised to ISO 8601 before the diff, so that reading a
  KDBX 3 database and writing KDBX 4.1 still compares.
- **`Meta/CustomData/Item/_LAST_MODIFIED`.** KeePassXC stamps it at export time.
  Two exports of the same untouched file already differ here.
- **Order of `CustomData/Item`, `CustomIcons/Icon` and `DeletedObjects`.** These
  are maps in the `keepass` crate, so it writes them in hash order. The elements
  are sorted by key before the diff.
- **`Meta/Binaries` and `Meta/SettingsChanged`.** Artefacts of the format
  version: KDBX 3 carries the first, KDBX 4 the second. Only their presence is
  ignored, never their contents.

Attachment bytes never appear in a KDBX 4 export, only a `Ref` attribute, so the
suite additionally exports every attachment from both databases with
`keepassxc-cli attachment-export` and compares the bytes.
