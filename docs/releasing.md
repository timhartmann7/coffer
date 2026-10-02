# Cutting a release

The workflow does the machine half. This is the half that needs a real Mac, a
real download and a second architecture, and the reasons the order is the order.

## Before the tag

The version is written in three files and they drift. Bump all three in one
commit:

- `Cargo.toml`, `[workspace.package] version`
- `crates/vault-gui/tauri.conf.json`, `version`
- `crates/vault-gui/Cargo.toml`, the `version` beside `vault-core`'s path

The third is there because `cargo deny` bans a wildcard dependency, and a path
with no version is one. Missing it does not fail the version job: it fails later,
inside the suite, as Cargo refusing to resolve a caret range, on a message that
names neither the tag nor the line. `frontend/src/lib/version.test.ts` is what
catches all three, and it runs long before a tag exists.

The bundler builds the asset names out of the second one, so a tauri.conf.json
that lags the tag produces `Coffer_0.1.0_aarch64.dmg` under a `v0.2.0` release
and a cask that points at a file the release does not carry. The `version` job in
`.github/workflows/release.yml` refuses the tag when the three disagree, and it
runs before the builds rather than after them.

Then push the tag. Nothing else starts a release.

```
git tag v0.2.0
git push origin v0.2.0
```

## What the workflow does

`version` agrees the three numbers. `suite` calls `ci.yml`, so the release is
gated on the same definition of green a branch push gets, on the exact commit the
tag names. `build` runs twice, each architecture on a runner of that
architecture: cross-compiling x86_64 from an arm host works, and it would make
the Intel disk image the one artefact nobody has ever run on the machine it
targets. `release` publishes the two images and `checksums.txt`. `cask` puts the
two hashes into `Casks/coffer.rb` and pushes that to `main`.

Two fallbacks worth knowing before you need them:

- **If `macos-26-intel` ever goes away**, drop the matrix to one `macos-26` job,
  `rustup target add x86_64-apple-darwin`, and run `cargo tauri build` twice.
  The Intel image is then cross-compiled, and the acceptance criterion moves
  entirely into the checks below.
- **If the `cask` job fails**, the release is still good. Edit three lines of
  `Casks/coffer.rb` by hand: the version, and the two hashes out of the
  release's own `checksums.txt`.

A second run on a tag that already has a release fails at `gh release create`,
and that is the right answer. Deleting a published release breaks every
`install.sh` mid-download and every cask already pointing at it.

## Building one by hand

`cargo tauri build` on a Mac with a window server leaves a copy of `Coffer.app`
in the real `/Applications`, without being asked. The disk image carries an
`Applications` symlink so that a reader can drag onto it, and the bundler's own
`bundle_dmg.sh` drives Finder to lay the window out; that pass copies through the
symlink. It is reproducible, and setting `CI=true` stops it, because that is what
makes the bundler skip the Finder step:

```
CI=true cargo tauri build --target aarch64-apple-darwin
```

The release workflow is unaffected, because Actions sets `CI` itself. Use the
same variable locally unless you want the copy, and check `/Applications`
afterwards if you did not.

## The checks a runner cannot make

Run these on a real machine before announcing anything. They are the pre-release
list for the release itself, and the first items are checks of the window's
own, which WebKit and AppKit answer and no runner has; the rest of the vault's
list is in the spec.

- A revealed value, selected: the Coffer menu has no Services, and Shift+Cmd+Y
  (New Sticky Note) and every other enabled Services shortcut put nothing in
  another application. A right-click on the label inside a selection that runs
  into the value draws no menu, and Cmd+C there leaves only the value's part,
  through Coffer's own copy. This file is where this check lives: the spec's
  list does not carry it.
- Every key of Coffer's items in the menu bar does its thing once: Cmd+N makes
  one entry, Cmd+F puts the focus in the search field, Cmd+B and Shift+Cmd+C copy
  once with one notice, Cmd+L locks, Cmd+, opens the settings, Shift+Cmd+N opens
  one folder name. Cmd+C with nothing selected still copies the password, and
  Cmd+Z still takes back a removal. The page's keys are answered before the menu
  is looked in, and this is the check that none is answered twice or not at all.
- Cmd+Backspace in Notes and in the search field deletes to the start of the line
  and moves nothing to the bin; with the focus on a row or a button it moves the
  open entry to the bin and offers it back. The menu draws the item with ⌫.
- Greying: on the unlock screen only Settings…, Open Vault… and Keyboard
  Shortcuts can be chosen, and on the creation screen only Open Vault… and
  Keyboard Shortcuts - Open Vault… not while the vault is being made; Lock Vault
  only with a vault open, and Open Vault… never then; Move to Recycle Bin grey
  in the bin, in a read-only vault, for a vault with no bin, and while a field
  has the focus, and offered again as soon as the vault has opened. A grey
  item's key does nothing.
- A title typed into an entry and not yet left, then Cmd+N: the title is saved
  before the new entry opens, and reopening the first entry shows it with no
  Unsaved mark. The same with Cmd+, and with Cmd+B while a saved login is being
  changed, which copies what was typed - also while a save of a vault carrying a
  large file is still running. A login typed into an empty field leaves Copy
  Login grey until the field is left. Cmd+B with the focus in the search field
  copies the login and leaves the focus there, so the next key types into it.
- Keyboard Shortcuts with the focus on a field's copy button: Escape, Close and
  a press on the veil each give the focus back to that button, and VoiceOver
  reads it; Help ▸ Keyboard Shortcuts and then Cmd+F puts the focus in the
  search field instead. With the sheet up, Shift+Tab to the title bar's Lock
  button and Cmd+C copies nothing and Escape puts the sheet away, not the
  entry.
- Drag a row of the list onto a folder, onto TextEdit and onto the Desktop: the
  folder takes it and the notice says so; TextEdit and the Finder take nothing
  and draw no drop badge, because nothing went to the drag pasteboard. The same
  for a folder dragged out of the tree. Rest on a folded folder: it opens. Escape
  mid-drag: nothing moves, and the pane and the search stay. A drag that ends
  over the empty part of the folders pane leaves the open entry where it was,
  and a row pressed without moving still opens.
- Choosing rows, with a trackpad and with a mouse: Cmd-click chooses a row and
  opens nothing, Shift-click chooses the run from the last row pressed, and
  Control-click chooses nothing. Cmd+A with the focus on a row, on the empty part
  of the list and on a folder chooses every row the list draws, selects no text
  of the page, and the Edit menu does not flash; in the search field and in a
  field of the entry it selects that field's text, and Edit ▸ Select All chosen
  with the pointer selects nothing visible. VoiceOver reads ", selected" after a
  chosen row's title, and says "5 selected" after Cmd+A and the new count after
  each Cmd-click, with the focus where it was.
- With several rows chosen: Cmd+Backspace and Edit ▸ Move to Recycle Bin move all
  of them once, with one notice whose Undo and ⌘Z bring all of them back; in the
  bin the item is grey and the bar's Delete forever… asks. Several chosen rows
  dragged onto a folder move together with one notice; a row that is not chosen
  drags alone and the choice stays. Add tag: Return puts the tag on once, and
  a Return that ends a kana conversion does not; a click elsewhere, the bar's
  own Delete included, lets the tag go and puts it on nothing; Cmd+Tab away and
  back leaves what was typed in the field, and a lock with it typed puts it on
  nothing.
- The line above an entry's title: a press opens the folder list with the keys
  in its filter, Return moves the entry, ⌘Z takes it back, and VoiceOver reads
  the line that is chosen. In "All entries", "+ Entry" and Cmd+N open "Put it
  in" on the folder the last new entry went into.
- Cmd+W and the red button, with a vault open and a value typed a moment before:
  the window goes, Coffer stays in the Dock, the lock file beside the vault is
  gone, the Dock icon brings back the unlock screen where the window was, and
  after unlocking the value is in the vault. The same with a value typed while a
  save of a vault carrying a large file is running, closed before the save
  ends: the window waits for the save, and the value is in the vault. Cmd+W
  while the password is still being derived: nothing stays open, and the Dock
  brings back the unlock screen.
- With no window: Open Vault… builds the window and opens the panel, Settings…
  builds it and opens the settings, every other item of Coffer's is grey, and
  Cmd+Q quits. A minimised window comes back from a click on the Dock, and a
  click while a lock is building the window again shows no white frame.
- With the window minimised and Coffer still the active application: Cmd+N,
  Cmd+Backspace, Cmd+B, Settings… and Keyboard Shortcuts each bring the window
  back and happen there, where the notice they raise can be read; Cmd+L locks,
  and the window that comes back asks for the password.
- Help keeps its search field, and Keyboard Shortcuts opens the sheet; Edit still
  ends with the items macOS adds (Start Dictation, Emoji & Symbols).
- Both disk images mount, and the application inside each launches on a Mac of
  that architecture. `file Coffer.app/Contents/MacOS/Coffer` says the
  architecture you expect. Run the Intel image on an Intel Mac, not under
  Rosetta: an artefact nobody has run on the machine it targets is not evidence.
- `install.sh` on a Mac with no Coffer: five actions to a saved entry, counted.
- `install.sh` again over the installed copy, with Coffer running. It refuses and
  says why. This is the one check that catches `mainBinaryName` going missing:
  without it the process is called `vault-gui` and the guard is a silent no-op.
- `install.sh` again with Coffer quit. It replaces cleanly, and `settings.json`,
  `last-database` and `generator.json` in
  `~/Library/Application Support/app.coffer.vault/` survive.
- A hand-edited digit in `checksums.txt`. The script prints both hashes, deletes
  the download, exits non-zero, and never reaches `hdiutil`. `/Applications` is
  untouched.
- `COFFER_APPDIR="$HOME/Applications"` on an account that cannot write
  `/Applications`.
- The disk image downloaded in a browser: confirm macOS refuses it, then confirm
  the `xattr` line in the README gets past it and the System Settings route is
  still where the README says. The second route is the one that needs watching.
  It exists only while `codesign --verify --deep --strict` passes on the bundle:
  a signature macOS rejects is reported as a damaged application, and a damaged
  application has no **Open Anyway** at all.
- `brew tap timhartmann7/coffer https://github.com/timhartmann7/coffer` and
  `brew install --cask coffer`, then open Coffer. It has to launch on the first
  try: Homebrew stamps its downloads with `com.apple.quarantine` and the cask's
  postflight is what takes it off again, so this is the check that the Homebrew
  route reaches a saved entry at all.
- Then `brew uninstall --cask --zap coffer` on a Mac with a real vault. The
  `.kdbx`, its ten `.bak` snapshots and any `.lock` are all still there, and only
  the five `app.coffer.vault` paths are gone.
- `brew style Casks/coffer.rb`, which is clean once the workflow has put the two
  real hashes in, and `brew audit --cask --online timhartmann7/coffer/coffer`
  through the tap the check above installed. `brew audit` no longer takes a path.
  Expect the signing finding, because the bundle is signed ad-hoc rather than
  with a Developer ID, and nothing else.
