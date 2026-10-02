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
  into the value draws Coffer's menu - Copy and Hide, nothing of WebKit's - and
  its Copy, like Cmd+C there, leaves only the value's part, through Coffer's own
  copy. This file is where this check lives: the spec's list does not carry it.
- A right-click on a row of either list, a folder in the tree and in the bin,
  the bin's row, a field of one's own, the password's row, a file and a
  revealed value draws Coffer's menu where the pointer is, and the row, folder
  or file is outlined until it closes. Control-click draws the same menu and
  does not also open the row or choose it. VO+Shift+M on a focused row draws it
  too. A right-click on "All entries", on blank space and on the unlock screen's
  words draws nothing.
- A revealed password with nothing selected, right-clicked in the middle of a
  word: Copy puts the whole password on the pasteboard (paste it somewhere
  private), and no word stays selected. With part of it selected and the
  right-click on that part, Copy puts that part; with the right-click beside the
  part, the whole password, and the part stays selected. A clipboard history
  keeps none of them. This is the check that WebKit selects the word before the
  page hears of the menu, which the guard is built on.
- The search field, a value being written, a folder's name being typed, the
  new password's field and the three master password fields in Settings still
  get WebKit's menu, with Cut, Copy and Paste; the last three get none while
  "Changing it…" is up. With part of a revealed password selected, a
  right-click in the search field never draws Look Up, Translate, Search or
  Share over it: no menu at all while the part stays selected, or the field's
  own menu if the press took the selection into the field. Write down which of
  the two WebKit does; the guard is built for both.
- Folders named `R&D`, `a[~~]b`, one whose name holds a right-to-left override
  (U+202E) before `xyz`, and one 200 characters long each read correctly in Move
  to, the last cut with an ellipsis, and the items after them read left to
  right. A vault 100 folders deep reaches the deepest through the submenus.
- Choose rows with Cmd-click and right-click one of them: the menu moves and
  deletes all of them, with one notice; right-click a row outside the choice:
  the menu is that row's, and the choice stays.
- A title typed into an entry and not yet left, then a right-click on another
  row and Copy Password: the title is saved, and the notice says Password
  copied. A new password typed into a Change field, then Delete Forever… or New
  Entry Here from a menu: nothing happens but the field's question.
- Five minutes idle with a menu open: the window goes when the menu is
  dismissed, and choosing an item from it first does nothing to the vault that
  comes back.
- A backup open to look at (Settings, Show, "Open to look") and a KDBX 3 vault
  holding files: a right-click on a row, a folder, the bin's row, a field and a
  file draws every change greyed out, and copying still works. Save to… on the
  file writes it out of the backup, and is grey in the KDBX 3 vault.
- Every key of Coffer's items in the menu bar does its thing once: Cmd+N makes
  one entry, Cmd+D one copy, Cmd+F puts the focus in the search field, Cmd+B and Shift+Cmd+C copy
  once with one notice, Cmd+L locks, Cmd+, opens the settings, Shift+Cmd+N opens
  one folder name. Cmd+C with nothing selected still copies the password, and
  Cmd+Z still takes back a removal. The page's keys are answered before the menu
  is looked in, and this is the check that none is answered twice or not at all.
- Cmd+Backspace in Notes and in the search field deletes to the start of the line
  and moves nothing to the bin; with the focus on a row or a button it moves the
  open entry to the bin and offers it back. The menu draws the item with ⌫.
- Greying: on the unlock screen only Settings…, Open Vault…, Show in Finder
  (while the file is there) and Keyboard Shortcuts can be chosen, and on the
  creation screen only Open Vault… and Keyboard Shortcuts - Open Vault… not while
  the vault is being made; Lock Vault and Save a Copy… only with a vault open,
  and Open Vault… never then; Save a Copy… grey in a backup, in a lock's copy,
  and in a KDB vault or a KDBX 3 one holding files; Move to Recycle Bin grey in
  the bin, in a read-only vault, for a vault with no bin, and while a field has
  the focus, and offered again as soon as the vault has opened. A grey item's
  key does nothing.
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
- "+ Entry"'s chevron: a press, and Down and Up on it with the keyboard, open
  the list of kinds with the focus on a line; the arrows go round it, Escape
  closes it with the focus back on the chevron and leaves the search and the
  pane alone - from a line, and from the chevron after Shift+Tab - a second
  press on the chevron closes it rather than flickering it shut and open again,
  and a press anywhere else closes it. Each kind made in a folder
  opens with its name selected, and in KeePassXC it draws the icon
  `docs/vault-core.md` names for it - the key, Money, IRCommunication,
  Identity, Package, PaperLocked, Note, TerminalEncrypted - carries its tag,
  and shows the fields that kind hides as hidden. A vault whose templates group
  KeePass 2 or MacPass named lists its templates after the kinds, and an entry
  made from one opens in KeePassXC with the template's files.
- Duplicate, from the pill and with Cmd+D: with the focus in the login and a
  word typed a moment before, the copy holds the word, and so does the
  original - which is the check that WebKit hands Cmd+D on from a focused text
  field to the menu rather than keeping it. A tag typed into the tag field, or
  a new name into a field's name, and Cmd+D pressed there: the copy carries the
  tag or the name. Cmd+D is grey with no entry open, in the bin, in a read-only
  vault, and with rows chosen by Cmd-click. A copy of an entry with a file and a
  custom icon opens in KeePassXC beside the original, with the file and the
  icon and no history, and removing the copy's file in KeePassXC leaves the
  original's.
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
- A master password changed in Settings, on a vault with snapshots and on one
  that needs a key file. After a lock, the new password opens it and the old one
  is refused. KeePassXC opens the vault with the new password (and the key
  file), and `vault.kdbx.1.bak` with the old one. With one more save in between,
  "Remove old backups" takes exactly the `.bak` files that were there when the
  password changed. The eye shows a password only while its field has the
  focus. WebKit offers neither AutoFill nor to save the password on any of the
  three fields. Escape and the Settings button do nothing while "Changing it…"
  is up. With VoiceOver on, "Master password changed.", "Removed 4 old
  backups." and a refusal are read out without moving the focus, and the eye is
  read as "Show the password, button" with nothing about being selected. Inside
  a lock's copy the settings have no Master password row until "Make this my
  vault"; with such a copy beside the vault the row offers no "Change…" and
  says what to do with the copy, and once the copy is removed or made the vault
  it offers the change again. A wrong current password takes about a second to
  be refused, and Lock Vault answers at once while it waits. Press "Change the
  password" and close the lid straight away: after waking, the unlock screen
  says the master password was changed before locking, the new password opens
  the vault, and the sentence is gone once it is open.
- Backups, on a vault saved a few times. Settings shows "N copies"; Show lists
  them to the minute in the Mac's own clock, newest first, with "Open now" on
  none of them. With a value typed into an entry and not yet left, press "Open
  to look" on the second: the window comes back on that backup's unlock screen
  ("A backup, not your vault"), and the typed value is in the vault when it is
  next opened. Unlock the backup: the strip is up, and "Read only" in the
  status bar, clicked with the mouse over an open entry, opens its note above
  it; Escape closes the note and leaves the entry open, and so does a click
  anywhere else. "Save a copy as…" writes a file KeePassXC opens; aimed at
  `vault.kdbx` in the panel and answered "Replace", it is refused with "there is
  already a file with that name" and the vault file is unchanged. "Use this copy as
  my vault": the notice names `….kdbx.1.bak`, the status bar no longer says Read
  only, and the vault opens with the same password after a lock. Damage the
  vault file (`printf 'x' | dd of=vault.kdbx bs=1 seek=4000 conv=notrunc`),
  unlock it: every backup is listed, and using one keeps the damaged file as
  `vault.kdbx.replaced-<today>.kdbx`, which is still there ten saves later.
  Change the master password, then make a backup from before the change the
  vault: the strip said the vault opens with the backup's password, the old
  password opens it, and the file it replaced opens with the new one in
  KeePassXC.
- Ten backups, a value typed into an entry and not yet left, then Settings,
  Show, "Open to look" on the tenth: the window comes back on the vault's unlock
  screen saying the backup is not there any more because the save on the way
  pushed it out. In Settings, change the master password and remove the old
  backups: the count under "Automatic backups" follows, and no row is left to
  open. With a backup open, press "Use this copy as my vault" and Cmd+L at once:
  the unlock screen says a backup was made the vault, which password opens it,
  and where the replaced file went.
- A vault on an exFAT stick, which keeps no hard links: list the backups,
  save the vault from a second Coffer once, then press "Open to look" on a row
  the settings showed before - it opens that backup or says it is gone and
  lists again, never its neighbour. Damage the vault file and use a backup: the
  old file is kept beside it as `….replaced-<date>.kdbx` (copied, not linked),
  and the vault opens.
- A vault on a read-only disk image (`hdiutil create -srcfolder … -format UDRO`):
  the status bar's Read only says the place, and "Save a copy somewhere else…"
  writes a copy that opens. A KDBX 3 vault holding files says so and offers no
  copy. With VoiceOver on, the Read only button is read as collapsed and
  expanded, and a backup's row button names the file.
- File ▸ Show in Finder (⌥⌘R), from the vault, from its settings and from the
  unlock screen, selects the vault's file in a Finder window and brings the
  Finder forward; Settings › Vault's "Show in Finder" does the same. A vault
  opened through a link selects the file it leads to. With the file moved away
  while the unlock screen is up, the item is grey once the screen is drawn
  again, and pressed before that it says the file is gone and lists the backups.
- File ▸ Save a Copy… (⇧⌘S) with a stick in and no copy made yet: the panel
  opens on the stick, offers `vault YYYY-MM-DD.kdbx`, the notice names the stick,
  the status bar's "No copy on another disk for N days" goes, and Settings says
  "Last made: today, on “Stick”". Save again the same day: the panel offers
  `vault YYYY-MM-DD 2.kdbx`. Pick the first name and answer "Replace": it is
  refused with "A file by that name is already there, so nothing was replaced",
  and the first copy is unchanged. Take the stick out and save again: the panel
  opens beside the vault, and the notice and Settings say that copy is on the
  same disk as the vault. Put the stick back: the panel opens in the folder on
  the stick used last. Every copy opens in Coffer and in KeePassXC with the
  vault's password.
- Only a copy on another disk resets the reminder. Quit Coffer, move the stick
  copy's `at` in `copies.json` back 31 days (86400 seconds a day), and launch:
  the status bar says "No copy on another disk for 31 days". Save a copy beside
  the vault: the line does not go, and Settings says that copy is on the same
  disk as the vault. Save one to the stick: the line goes.
- Coffer's own disk image mounted and a Time Machine disk connected: neither is
  where the panel opens.
- A writable disk image kept on the Mac's own disk (`hdiutil create -size 50m
  -fs APFS -volname Private -type SPARSEBUNDLE`, then open it), and a second
  APFS container or partition on the internal disk where the Mac has one:
  neither is where the panel opens, and a copy saved into either is said to be
  on the same disk as the vault.
- Save a Copy… to a stick that is pulled out, or a share whose server is
  switched off, while "Saving a copy…" is up (a vault holding a few hundred
  megabytes of files gives the time): Lock Vault and Cmd+L lock at once, and
  Quit quits, without waiting for the disk. The copy is refused or written
  whole, never left half written under its name.
- A share mounted from another Mac, which is then switched off: Save a Copy…
  still opens its panel. When the last copy went to that share, write down how
  long the panel takes to appear.
- A vault with entries made more than thirty days ago (one from another client
  does): the unlocked status bar says "No copy on another disk for N days · Save
  a copy" in its own capitals, the path gives way first at the window's smallest
  width and the bar stays one line, the unlock screen says nothing, and the line
  goes after a copy to a stick. With a lock while the panel is up, the window
  that comes back is the unlock screen and nothing is written.
- Neither ⇧⌘S nor ⌥⌘R is taken by the page while the focus is in a field, and
  each happens once.
- Both disk images mount, and the application inside each launches on a Mac of
  that architecture. `file Coffer.app/Contents/MacOS/Coffer` says the
  architecture you expect. Run the Intel image on an Intel Mac, not under
  Rosetta: an artefact nobody has run on the machine it targets is not evidence.
- `install.sh` on a Mac with no Coffer: five actions to a saved entry, counted.
- `install.sh` again over the installed copy, with Coffer running. It refuses and
  says why. This is the one check that catches `mainBinaryName` going missing:
  without it the process is called `vault-gui` and the guard is a silent no-op.
- `install.sh` again with Coffer quit. It replaces cleanly, and `settings.json`,
  `last-database`, `generator.json` and `copies.json` in
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
