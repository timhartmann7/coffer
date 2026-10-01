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
list for the release itself, and the first item is a check of the vault's own;
the rest of the vault's list is in the spec.

- A revealed value, selected: the Coffer menu has no Services, and Shift+Cmd+Y
  (New Sticky Note) and every other enabled Services shortcut put nothing in
  another application. A right-click on the label inside a selection that runs
  into the value draws no menu, and Cmd+C there leaves only the value's part,
  through Coffer's own copy. This file is where this check lives: the spec's
  list does not carry it.

- Both disk images mount, and the application inside each launches on a Mac of
  that architecture. `file Coffer.app/Contents/MacOS/Coffer` says the
  architecture you expect. Run the Intel image on an Intel Mac, not under
  Rosetta: an artefact nobody has run on the machine it targets is not evidence.
- `install.sh` on a Mac with no Coffer: five actions to a saved entry, counted.
- `install.sh` again over the installed copy, with Coffer running. It refuses and
  says why. This is the one check that catches `mainBinaryName` going missing:
  without it the process is called `vault-gui` and the guard is a silent no-op.
- `install.sh` again with Coffer quit. It replaces cleanly, and `settings.json`
  and `last-database` in `~/Library/Application Support/app.coffer.vault/`
  survive.
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
