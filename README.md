# Coffer

Coffer keeps passwords, logins, SSH keys and anything else that has to stay
secret in one encrypted file on your Mac. One master password opens it. There is
no account, no sync and no server: the file is yours, and it does not leave the
machine unless you move it.

macOS 13.3 and later. Apple Silicon and Intel, as two separate builds.

## Install

```
curl -fsSL -O https://raw.githubusercontent.com/timhartmann7/coffer/main/install.sh && bash install.sh
```

That finds the newest release, picks the build from `uname -m`, checks the
download against the SHA-256 published beside it, and puts `Coffer.app` in
`/Applications`. The script is short and it is in this repository; run only the
half before the `&&` if you would rather read it before you run it.

It is deliberately not a `curl | sh`. Nothing here should ask you to execute
something you cannot see, and a transfer that breaks halfway into a shell runs
half a script.

Or through Homebrew:

```
brew tap timhartmann7/coffer https://github.com/timhartmann7/coffer
brew install --cask coffer
```

### Five actions to the first saved entry

1. Run the line above.
2. Open Coffer from `/Applications`.
3. Press **Make a vault**.
4. Type a master password twice and press return.
5. Press **+** and type the entry. There is no save button: it is on disk as
   soon as it is typed.

The vault goes to `~/Coffer/vault.kdbx` unless you say otherwise on step 4, and
you can move the file afterwards like any other file.

### If you download the disk image by hand

The builds are not signed with a Developer ID and they are not notarised. There
is no certificate behind this project.

That is invisible when `install.sh` or Homebrew does the fetching, because
`curl` does not mark what it downloads. A browser does: it writes a quarantine
attribute onto the disk image, macOS refuses the first launch, and since
macOS 15 the right-click-and-Open way past that is gone. If Coffer is in
`/Applications` and will not open, either

```
xattr -dr com.apple.quarantine /Applications/Coffer.app
```

or open **System Settings → Privacy & Security**, find Coffer under
**Security**, and press **Open Anyway**. On macOS 26 that button goes away about
an hour after the refusal, so the command is the reliable one.

## What Coffer promises

- **The file is not ours.** It is KDBX 4.1. KeePassXC, KeePassium, KeePass and
  every other client of that format open it, now and after this project stops.
- **You can leave at any moment, and take the file.** There is nothing to
  export, because there is nothing else.
- **No network requests, ever.** No updater, no analytics, no crash reporting.
  Nothing that can open a socket is in the dependency graph, and the build fails
  if one appears.
- **No fields of our own.** The moment Coffer wrote something the specification
  does not have, the first promise would stop being true.

## What Coffer does not protect against

- **Code already running as you.** It can read this process's memory and record
  the screen. No local vault protects against that, and Coffer does not pretend
  to.
- **A forgotten master password.** It is not kept anywhere and it is not sent
  anywhere. There is no recovery, from us or from anybody.
- **Universal Clipboard.** A password you copy reaches your other Apple devices
  if Universal Clipboard is on. macOS offers no documented way for an
  application to keep an item out of it. Turn it off under **System Settings →
  General → AirDrop & Handoff** if that matters.

## What it does with your file

- Argon2id, measured on your Mac when the vault is made, so that unlocking takes
  about a second on the machine that will be unlocking it. Somebody else's
  database always opens with its own parameters.
- Every write is staged beside the file and renamed into place, so a crash
  halfway leaves the old database whole.
- Ten rotating snapshots, `vault.kdbx.1.bak` through `vault.kdbx.10.bak`, beside
  the database. The database and every snapshot are `0600`.
- An advisory `vault.kdbx.lock` while the vault is open, in the shape other
  clients of the format write, and a dialog rather than a silent overwrite when
  the file changed underneath.
- Previous versions of an entry live in the file's own history block, pruned to
  the limits the file carries.
- The vault locks itself after five minutes idle, on sleep and on screen lock.
  Locking destroys the window and wipes the decrypted tree; it does not hide
  anything.
- A copied password is written as a concealed, transient item, which keeps it
  out of clipboard history tools, and cleared after a minute.

Coffer keeps two things of its own, both in
`~/Library/Application Support/app.coffer.vault/`: `settings.json` and
`last-database`. Nothing else, and nothing anywhere else.

## Building it

The current stable Rust toolchain, Node 24, and the Tauri CLI.

```
cd frontend && npm ci && cd ..
cargo tauri build --target aarch64-apple-darwin
cargo tauri build --target x86_64-apple-darwin
```

Each leaves `target/<triple>/release/bundle/dmg/Coffer_<version>_<arch>.dmg`.
There is no universal binary: merging the two halves into one file doubles the
weight for nothing anyone notices.

`docs/vault-core.md` describes the engine and what the format library underneath
it gets wrong. `docs/ipc.md` is the contract between the engine and the window,
and the rule that keeps secrets on the Rust side of it. `docs/releasing.md` is
how a release is cut.

## Licence

Apache 2.0. See `LICENSE`.
