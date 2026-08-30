cask "coffer" do
  arch arm: "aarch64", intel: "x64"

  version "0.1.0"
  sha256 arm:   "0000000000000000000000000000000000000000000000000000000000000000",
         intel: "0000000000000000000000000000000000000000000000000000000000000000"

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
  depends_on macos: ">= :ventura"

  app "Coffer.app"

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
    Coffer is not signed with a Developer ID and is not notarised. Homebrew
    fetches with curl, which marks nothing, so it opens on the first try.
  EOS
end
