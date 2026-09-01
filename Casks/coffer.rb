cask "coffer" do
  arch arm: "aarch64", intel: "x64"

  version "0.1.0"
  sha256 arm:   "f1db374792928a0e40bb64ecc27a45f64ee7c0445b6ab9a5620afa4fbfbc498d",
         intel: "5be746978aefdaff27d8b177e7033b0dfde50aef1d9c00af1d8a521a2434f184"

  url "https://github.com/timhartmann7/coffer/releases/download/v#{version}/Coffer_#{version}_#{arch}.dmg"
  name "Coffer"
  desc "Local secrets vault in a single encrypted KDBX 4.1 file"
  homepage "https://github.com/timhartmann7/coffer"

  livecheck do
    url :url
    strategy :github_latest
  end

  # The bundle asks for 13.3. Homebrew names whole releases, so this is the
  # nearest thing it can say.
  depends_on macos: :ventura

  app "Coffer.app"

  # Homebrew stamps every cask download with com.apple.quarantine, and Coffer is
  # not signed with a Developer ID, so macOS would refuse the first launch and
  # since macOS 15 there is no right click past it. This is the same line the
  # README gives a reader who fetched the disk image by hand, run here so that
  # both documented ways in behave the same. It is written out rather than hidden
  # because a cask that quietly strips a security attribute is exactly the shape
  # of one that should not be trusted.
  postflight do
    system_command "/usr/bin/xattr",
                   args: ["-dr", "com.apple.quarantine", "#{appdir}/Coffer.app"]
  end

  # An AppleEvent rather than a signal, so Coffer runs its own exit: the lock
  # file beside the vault goes and the clipboard is cleared. Nothing is lost by
  # quitting, because there is no save button and every edit is already written.
  uninstall quit: "app.coffer.vault"

  # The vault is wherever its owner put it, and so are its ten .bak snapshots.
  # Nothing below goes near either: this is only what Coffer and the webview
  # write for themselves.
  zap trash: [
    "~/Library/Application Support/app.coffer.vault",
    "~/Library/Caches/app.coffer.vault",
    "~/Library/HTTPStorages/app.coffer.vault",
    "~/Library/Saved Application State/app.coffer.vault.savedState",
    "~/Library/WebKit/app.coffer.vault",
  ]

  caveats <<~EOS
    Coffer is not signed with a Developer ID and is not notarised. Homebrew marks
    what it downloads, so this cask takes the quarantine attribute back off
    Coffer.app after installing it; without that macOS would refuse to open it
    and offer no way through. The line it runs is in the cask.
  EOS
end
