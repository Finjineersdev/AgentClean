# Releasing OpenSweep

1. Make sure `main` is green (CI and engine workflows).
2. Tag and push. The tag sets the version; `MARKETING_VERSION` in the project
   is only the default for local builds.
   ```bash
   git tag -a v0.4.0 -m "OpenSweep 0.4.0"
   git push origin v0.4.0
   ```
   A tag with a suffix (`v0.4.0-beta.1`) is published as a pre-release, which
   the update feed and the cask skip.
3. The **Release** workflow runs the engine tests and the no-delete check,
   builds a universal app, signs it (Developer ID and notarization when those
   secrets exist), writes a sha256, an SBOM and build provenance, signs a
   Sparkle appcast, and publishes the GitHub Release. The tap follows it.

OpenSweep ships without Apple Developer ID (an open-source choice): the app
is ad-hoc signed and not notarized, and the release is a normal release, so
the update feed and the Homebrew cask pick it up. The README tells people how
to open it the first time. Add the Developer ID secrets below later and the
same workflow signs, notarizes and staples instead.

## One-time setup (owner)

| Kind | Name | What |
| --- | --- | --- |
| Secret | `SPARKLE_PRIVATE_KEY` | Sparkle EdDSA private key (see below) |
| Variable | `SPARKLE_PUBLIC_KEY` | The matching public key; it is built into the app |
| Optional secrets | `MACOS_CERT_P12`, `MACOS_CERT_PASSWORD`, `MACOS_SIGN_IDENTITY`, `APPLE_TEAM_ID`, `APPLE_ID`, `APPLE_APP_PASSWORD` | Developer ID signing and notarization, only if you join the Apple Developer Program |

Sparkle keys: run, once, as yourself (not with sudo):

```bash
scripts/setup-sparkle-key.sh
```

It fetches Sparkle's tools if needed, creates the key in your login keychain
under `com.finjineers.opensweep`, and stores the public key as the
`SPARKLE_PUBLIC_KEY` variable and the private key as the
`SPARKLE_PRIVATE_KEY` secret.

The login keychain is the backup: without the private key, installed copies
can never be updated again, so keep your keychain backed up (Time Machine).

Homebrew tap: [`Finjineersdev/homebrew-tap`](https://github.com/Finjineersdev/homebrew-tap)
holds `Casks/opensweep.rb`. Its own workflow checks the latest release every
six hours (or on demand from its Actions tab) and updates version and sha256,
so this repository needs no token for it.

## Updates

The app checks `releases/latest/download/appcast.xml` of this repository, so
the repository must be public. Sparkle asks the user before its first
automatic check, sends no system profile, and the updater is the app's only
network use. Local and ad-hoc builds carry no key and never check.

## Homebrew

```bash
brew install --cask finjineersdev/tap/opensweep   # app, plus the opensweep command
brew upgrade --cask opensweep
brew uninstall --zap --cask opensweep             # also deletes settings and the quarantine
```

Restore or purge every run before `--zap`: it removes the quarantine with
whatever is still held there.

## Local release build

```bash
xcodebuild -project OpenSweep.xcodeproj -scheme OpenSweep -configuration Release \
  -derivedDataPath build/dd build CODE_SIGN_IDENTITY=-
```
