# Template for Casks/agentclean.rb in the Finjineersdev/homebrew-tap repository.
# The Release workflow fills in version and sha256 and pushes it there.
cask "agentclean" do
  version "0.0.0"
  sha256 "0000000000000000000000000000000000000000000000000000000000000000"

  url "https://github.com/Finjineersdev/AgentClean/releases/download/v#{version}/AgentClean-#{version}.dmg"
  name "AgentClean"
  desc "Developer disk cleaner that quarantines rebuildable project folders"
  homepage "https://github.com/Finjineersdev/AgentClean"

  livecheck do
    url :url
    strategy :github_latest
  end

  auto_updates true
  depends_on macos: ">= :sonoma"

  app "AgentClean.app"
  binary "#{appdir}/AgentClean.app/Contents/Helpers/agentclean"

  # Zap also removes the quarantine: restore or purge every run first.
  zap trash: [
    "~/Library/Application Support/AgentClean",
    "~/Library/Caches/com.finjineers.agentclean",
    "~/Library/Preferences/com.finjineers.agentclean.plist",
  ]
end
