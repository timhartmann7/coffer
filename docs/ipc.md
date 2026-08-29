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
| Tree and list | id, title, username, URL, tags, dates, whether there is a password, how many attachments |
| Open an entry | the same, plus every field's name, kind and whether it is empty, plus attachment names and sizes |
| Reveal a field | the value, one field, once |
| Read a previous version | the same as an entry, and one value at a time on a reveal |
| Copy a field | nothing; Rust writes the pasteboard |
| Open an address | nothing; Rust hands the URL to the system |
| Add a file | nothing; Rust opens the panel and reads the file |
| Write a file out | nothing; Rust opens the panel and writes it |
| Make a password | the password, once, the way a reveal answers |

A field's `value` is `null` exactly when it does not cross: the database
protects it, or it is the password. **A password never crosses, protected or
not.** A file can hold a `Password` field the database left unprotected, and it
is still a password; `empty` says whether there is one to ask for.

## The commands

| Command | Takes | Answers |
|---|---|---|
| `status` | | the chosen database, whether it is open, how many entries, whether there is an unsaved change, whether it can be written |
| `choose_database` | | the database the user picked, or nothing if they closed the dialog |
| `unlock` | the master password, as the raw body | nothing |
| `lock` | | nothing |
| `tree` | | the root group, its sections and their entry rows |
| `entry` | `id` | one entry's fields, attachments, tags, dates and version count |
| `reveal` | `entry`, `field` | the value of that field |
| `copy` | `entry`, `field` | the seconds until Coffer clears the pasteboard |
| `open_url` | `entry` | nothing |
| `snapshots` | | the `.bak` files beside the chosen database, newest first |
| `choose_snapshot` | `index` | the snapshot now chosen |

Everything slice 3 added:

| Command | Takes | Answers |
|---|---|---|
| `create_entry` | `group` | the tree, and the entry it made |
| `delete_entry` | `entry` | the tree |
| `create_group` | `parent`, `name` | the tree |
| `rename_group` | `group`, `name` | the tree |
| `delete_group` | `group` | the tree |
| `empty_recycle_bin` | | the tree |
| `set_field` | `entry`, `field`, `value`, `protect` | the entry |
| `remove_field` | `entry`, `field` | the entry |
| `set_tags` | `entry`, `tags` | the entry |
| `add_attachment` | `entry` | the entry |
| `export_attachment` | `entry`, `name` | nothing |
| `remove_attachment` | `entry`, `name` | the entry |
| `versions` | `entry` | the previous versions, oldest first |
| `version` | `entry`, `index` | one version, read like an entry |
| `reveal_version` | `entry`, `index`, `field` | the value of that field in that version |
| `restore_version` | `entry`, `index` | the entry |
| `delete_version` | `entry`, `index` | the versions that are left |
| `clear_history` | `entry` | the versions that are left, which is none |
| `generate_password` | `length`, `alphabets`, `similar` | a password |
| `save` | | nothing |
| `save_over` | | nothing |
| `save_copy` | | the file it wrote, or nothing if the panel was closed |
| `reload` | | the tree |
| `rival` | | when the file on disk was written and how many entries it holds |

**A command that changes something answers with what it changed.** A change to
one entry answers with that entry; a change to the shape of the vault answers
with the whole tree. The screen never patches its own copy of the database from
what it thinks a command did, because a screen that guessed wrong would go on
drawing something that is not in the file.

**A version is addressed by `(entry, index)`.** The index is its position in the
entry's history, which is the only thing that identifies one: modification times
have one-second resolution, so two versions written in the same second are
indistinguishable by date, and the list is ordered by date rather than by
position because a file another client wrote may hold them in any order.

**`add_attachment` and `export_attachment` open their panel in Rust.** The
bytes of a file never cross in either direction and neither does a path: the
webview asks, the reader picks, and Rust reads or writes. The name a save panel
is offered comes from
[`Attachment::file_name`](../crates/vault-core/src/model.rs), which is the one
place a name out of a database is turned into a file name.

Nothing that names a file comes from the webview. `choose_database` opens the
system's own dialog and keeps the answer; `choose_snapshot` takes a slot number
and builds the path from the database the user already chose. There is no
command that opens a path the frontend sends.

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
`noVault`, `noSuchEntry`, `refused`, `io`, `other`.

`attachmentInHistory` is the one the entry screen has an answer for. Removing a
file is refused while previous versions of the entry still hold it - the format
keeps them inside the entry and nothing can rewrite one - so the screen offers
to clear those versions and remove the file, which is the only sequence that
works and is the reader's to choose.

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

A field the reader is editing crosses as ordinary JSON, and the master password
does not. The difference is not carelessness, it is what the two values are.

A password being edited is in the window already: it is the `value` of an input
element the reader is typing into, which is a JavaScript string by construction,
and no shape of message changes that. The master password is different in one
way that matters - it is never displayed, never edited in place, and it opens
everything - so it is worth the machinery of a raw body, and `unlock` is the
only command that gets it.

What the boundary is for still holds in both directions: a value the database
protects **leaves** the vault only through `reveal`, `reveal_version` or
`generate_password`, one value at a time and only when the screen asked.
`set_field` carries `protect`, which the screen read off the field it is
editing, so a value the database keeps protected goes back protected rather than
being written into the file as plain text.

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

**What Coffer cannot do about the copy Tauri keeps.** WebKit copies the request
body once, Tauri moves it into an `InvokeBody::Raw(Vec<u8>)`, and that buffer is
dropped without being wiped. `Zeroizing` on this side covers the copy Coffer
takes and nothing else. The residue is documented rather than fixed because
there is no way to reach it.

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
- **The generator's slider and the length the engine accepts are written
  twice.** `LENGTHS` in `generate.rs` is the eight to sixty-four the slider
  offers. The engine brings anything outside it back inside rather than trusting
  it, because a length the screen could not have asked for came from something
  that is not the screen. Change one and change the other.
- **The two bars that drain are CSS animations of a fixed length, and their
  lengths are written twice.** `--animate-drain-reveal` in `app.css` is the
  thirty seconds of `SECONDS` in `reveal.svelte.ts`; `--animate-drain-clipboard`
  is the sixty of `clipboard::CLEAR_AFTER` in Rust. Neither pair can share a
  value, because the only way to set a duration from script is an inline style
  and the policy below forbids one. Change one and change the other.
- **Nothing in the window may use an inline style.** `style-src 'self'` covers
  `style` attributes as well as `<style>` elements, and Tauri's nonce only
  reaches elements that are in the HTML at build time. This is why the sprite is
  hidden with a class, and why the two timers that drain on screen are CSS
  animations of a fixed length rather than a width that JavaScript sets.

## What the window does not do yet

**It draws every row it is given.** The list has no windowing: a vault of a
thousand entries, which is what `SPEC.md` sets a time budget for, draws in one
pass and filters on the text it folded when the folder was opened. Fifty
thousand would draw fifty thousand rows and take its time about it. The filter
itself is tested at that size; the drawing is not, and the fix when it matters is
to draw only the rows on screen.

**A snapshot opened from the unlock screen becomes the chosen database for the
rest of the session,** and it is read only. Writing to a `.bak` would put the
change in a file the next save of the database beside it rotates away, so every
change is refused with `readOnly` and `save_copy` is the way out: it writes what
is in the window to a file of the reader's choosing, which then opens like any
other database.

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

**The window is not destroyed on lock.** `SPEC.md` and slice 4 ask for that.
Slice 2 drops the vault, which wipes the decrypted tree, the master password and
the lock file, and returns the window to the unlock screen. Destroying the last
window today would end the process, because the runtime treats it as the
application closing; slice 4 adds the handler that prevents it and rebuilds the
window.

**The Tauri crate is `crates/vault-gui`, not `src-tauri`.** `SPEC.md` names
`src-tauri/capabilities/` when it describes capabilities, which is the framework's
default layout. Coffer has one workspace with two crates, as the same document
says two paragraphs earlier, so the capability files, the configuration and the
window icon live in the crate that runs `tauri_build::build()`.

**TypeScript is pinned to 6.** `CLAUDE.md` asks for the newest major of every npm
dependency, and 7.0.2 is out. `svelte-check` 4.7.6 accepts `^5 || ^6`, so
TypeScript 7 would take `npm run check` out of the build. It moves when
`svelte-check` moves.

## Running it

```bash
cd crates/vault-gui
cargo tauri dev          # starts vite on 1420 and opens the window
cargo tauri build        # the same, as a release binary
```

The window icon at `crates/vault-gui/icons/icon.png` is generated from the brand
file with `assets/rasterise.py`. It exists because `generate_context!` will not
compile without one. The full icon set, the `.icns` and the installer are slice
5's.
