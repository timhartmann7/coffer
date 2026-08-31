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
| `remove_attachment_and_versions` | `entry`, `name` | the entry |
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

Everything slice 4 added:

| Command | Takes | Answers |
|---|---|---|
| `settings` | | the two timers, the two switches, the look, and the values each may be set to |
| `set_settings` | `settings` | what was actually stored, which is not always what was sent |
| `stirred` | | the seconds the open vault has left, or nothing when none is open |
| `default_new_database` | | where a first vault goes when nobody has said |
| `choose_new_database` | | where the reader wants the new vault instead |
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
picker is reachable from a screen a vault can be open behind.

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

**`stirred` is how the countdown stays honest, and it is deliberately rare.** The
window sends it on real input and at most once every fifteen seconds, and ticks
the number itself in between. A status bar that asked once a second would be an
idle timer resetting itself, and a vault that never locks.

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

**A save moves those positions,** because it brings every entry's history inside
the database's limits, so a list read before a save names versions that are no
longer there. The window reads the list back after the save rather than before
it, which is why every change is `save` and then `versions` and never the other
way round.

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
works and is the reader's to choose. That offer is `remove_attachment_and_versions`
rather than the two commands one after the other: a removal can be refused for
more than one reason, and the versions go back if the file still cannot go.

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

## Locking

Locking destroys the window. It does not hide one: a hidden window is a webview
still holding every value the reader looked at, in a heap nothing in this
process can reach.

One route in, whatever asked. The idle deadline, the machine's own
notifications and the button all post to
[`autolock::timer`](../crates/vault-gui/src/autolock/timer.rs), which is the
only caller of [`lock.rs`](../crates/vault-gui/src/lock.rs). That order is the
whole of what `lock.rs` is: the tree is wiped **first and synchronously**,
because destroying a window is a message to the event loop and a Mac going to
sleep will not wait for it; then the clipboard is taken back; then the stack the
key was derived on is written over; and only then is the destroy queued.

`Session::lock` answers whether it actually dropped a vault, and exactly one
caller is told yes. Tauri goes on handing out the window between a destroy being
queued and the event that says it happened, so a second trigger arriving at the
same moment would otherwise destroy the window the first one's rebuild had just
made.

The window is built again in the `Destroyed` event of the run callback and
nowhere else. A destroy followed by a build in one function always fails: the
label is taken until the event is delivered. It is built from its entry in
`tauri.conf.json` rather than by hand, so the rebuilt window is the same window,
and it is put back where the reader left it.

**No Tauri event goes the other way.** The rebuilt page asks `status` and gets
the reason it is asking for a password again. An event would need two
capabilities the window does not have, and Tauri never clears a destroyed
window's listeners: a rebuilt window reuses the label `main`, so every emit
after the first relock would serialise every dead listener id ever registered.

## What the window does not do yet

**It draws every row it is given.** The list has no windowing: a vault of a
thousand entries, which is what `SPEC.md` sets a time budget for, draws in one
pass and filters on the text it folded when the folder was opened. Fifty
thousand would draw fifty thousand rows and take its time about it. The filter
itself is tested at that size; the drawing is not, and the fix when it matters is
to draw only the rows on screen.

**The command layer itself has no tests.** What is under it does:
`Session` is covered, `vault-core` is covered, and `contract.test.ts` checks
that every command the window calls exists in Rust, is in the handler list, and
takes exactly the arguments the window sends. What is untested is the body of a
command - the dialog it opens, the thread it moves work onto. Testing one needs
Tauri's mock runtime and belongs with the window work rather than with this
slice.

**An entry cannot be moved between folders.** It is made in the folder that is
open and it stays there until it is deleted, which moves it to the recycle bin.
The mockup shows dragging a row into another folder; nothing here does that yet.

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

**A value on the screen can be selected, and a selection is not Coffer's
clipboard.** The boundary above says a copy is Rust's: the value reaches the
pasteboard marked concealed and transient, and Coffer takes it off again after a
minute. A reader who selects a revealed value by hand and presses the system's
own copy gets none of that - no concealed type, no auto-clear, and the clipboard
history tools keep it.

It is still what the window offers, because a note, an address and a field of
somebody's own have no copy button of their own, and a value that can be read on
the screen and not copied off it is a value the reader retypes. So `app.css`
names exactly three things that may hold a selection - a text field, a text
area, and the node a reveal writes into - and everything else in the window
paints no selection at all. `Cmd+C` steps aside when there is one, which is the
only reason the guard in `Vault.svelte` exists.

What it is not is a way around the boundary: nothing crosses IPC that did not
cross for the reveal, and a reader who can select a value can already read it.
The README's list of what Coffer does not protect against is where this belongs
when that list is next written.

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
