# Releasing AgentClean

1. Make sure `main` is green (CI and engine workflows).
2. Tag and push. The tag sets the version; `MARKETING_VERSION` in the project
   is only the default for local builds.
   ```bash
   git tag -a v0.4.0 -m "AgentClean 0.4.0"
   git push origin v0.4.0
   ```
   A tag with a suffix (`v0.4.0-beta.1`) is published as a pre-release and
   does not bump the cask.
3. The **Release** workflow runs the engine tests and the no-delete check,
   builds a universal app, signs it with hardened runtime (the bundled engine
   too), notarizes and staples the DMG, writes a sha256, signs a Sparkle
   appcast, publishes the GitHub Release, and bumps the Homebrew cask.

Without the Developer ID secrets the workflow still publishes, but as an
ad-hoc signed **pre-release** with no appcast and no cask bump.

## One-time setup (owner)

These need your Apple Developer account and keys, so an agent cannot do
them.

| Kind | Name | What |
| --- | --- | --- |
| Secret | `MACOS_CERT_P12` | `base64 -i DeveloperID.p12` of your **Developer ID Application** certificate, exported with its private key |
| Secret | `MACOS_CERT_PASSWORD` | The password used for that export |
| Secret | `MACOS_SIGN_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID)` |
| Secret | `APPLE_TEAM_ID` | Your 10-character team ID |
| Secret | `APPLE_ID` | Apple ID of the developer account |
| Secret | `APPLE_APP_PASSWORD` | An app-specific password from account.apple.com, for `notarytool` |
| Secret | `SPARKLE_PRIVATE_KEY` | Sparkle EdDSA private key (see below) |
| Variable | `SPARKLE_PUBLIC_KEY` | The matching public key; it is built into the app |
| Secret | `TAP_GITHUB_TOKEN` | Fine-grained token with **Contents: read and write** on `Finjineersdev/homebrew-tap` |

Sparkle keys: build the app once so Xcode fetches Sparkle, then

```bash
build/dd/SourcePackages/artifacts/sparkle/Sparkle/bin/generate_keys            # stores the key in your login keychain, prints the public key
build/dd/SourcePackages/artifacts/sparkle/Sparkle/bin/generate_keys -x sparkle.key   # export the private key for the secret, then delete the file
```

Keep a backup of the private key: without it, installed copies can never be
updated again.

Homebrew tap: create the public repository `Finjineersdev/homebrew-tap` (an
empty `README.md` is enough). The workflow writes `Casks/agentclean.rb`
there from `packaging/homebrew/agentclean.rb`.

## Updates

The app checks `releases/latest/download/appcast.xml` of this repository, so
the repository must be public. Sparkle asks the user before its first
automatic check, sends no system profile, and the updater is the app's only
network use. Local and ad-hoc builds carry no key and never check.

## Homebrew

```bash
brew install --cask finjineersdev/tap/agentclean   # app, plus the agentclean command
brew upgrade --cask agentclean
brew uninstall --zap --cask agentclean             # also deletes settings and the quarantine
```

Restore or purge every run before `--zap`: it removes the quarantine with
whatever is still held there.

## Local release build

```bash
xcodebuild -project AgentClean.xcodeproj -scheme AgentClean -configuration Release \
  -derivedDataPath build/dd build CODE_SIGN_IDENTITY=-
```
