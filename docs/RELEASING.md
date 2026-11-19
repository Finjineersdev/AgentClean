# Releasing AgentClean

1. Bump `MARKETING_VERSION` in `AgentClean.xcodeproj/project.pbxproj` (and `CURRENT_PROJECT_VERSION` if you like).
2. Commit, then tag and push:
   ```bash
   git tag -a v1.1.0 -m "AgentClean 1.1.0"
   git push origin main v1.1.0
   ```
3. The **Release** workflow builds the app on a macOS 26 runner, packages `AgentClean-<version>.dmg`, writes a sha256, and publishes a GitHub Release with generated notes.
4. With `TAP_GITHUB_TOKEN` set, it also bumps `Casks/agentclean.rb` in `Finjineersdev/homebrew-tap`, so `brew upgrade --cask agentclean` picks the new version up within the hour.

## Repository secrets

| Secret | Purpose | Required |
|---|---|---|
| `TAP_GITHUB_TOKEN` | Fine-grained PAT, repository `Finjineersdev/homebrew-tap`, permission **Contents: read and write**. Lets the workflow bump the cask. | For the cask auto-bump |
| `MACOS_CERT_P12` | `base64 -i DeveloperID.p12` of your **Developer ID Application** certificate (exported from Keychain Access with its private key) | For notarized builds |
| `MACOS_CERT_PASSWORD` | Password used when exporting the `.p12` | " |
| `MACOS_SIGN_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID)` | " |
| `APPLE_TEAM_ID` | Your 10-character team ID | " |
| `APPLE_ID` | Apple ID email of the developer account | " |
| `APPLE_APP_PASSWORD` | An app-specific password from appleid.apple.com, used by `notarytool` | " |

Without the certificate secrets the workflow ships an ad-hoc signed build; users open it with right-click → Open or `xattr -d com.apple.quarantine`. With them, the DMG is signed with hardened runtime, notarized, and stapled, and opens without warnings.

## Homebrew

Tap: https://github.com/Finjineersdev/homebrew-tap

```bash
brew install --cask Finjineersdev/tap/agentclean
brew upgrade --cask agentclean
brew uninstall --zap --cask agentclean   # also removes settings, logs, quarantine
```

To submit to the official `homebrew-cask` repo later: the app should be notarized and have a few releases behind it; then `brew bump-cask-pr` from a copy of this cask.

## Local release build

```bash
xcodebuild -project AgentClean.xcodeproj -scheme AgentClean -configuration Release -derivedDataPath build/rel build
mkdir -p dist/root && cp -R build/rel/Build/Products/Release/AgentClean.app dist/root/ && ln -s /Applications dist/root/Applications
hdiutil create -volname "AgentClean X.Y.Z" -srcfolder dist/root -ov -format UDZO dist/AgentClean-X.Y.Z.dmg
```
