# AgentClean handoff

Written 19 Nov 2026 at the end of milestone M1, to move the work from a
claude.ai session into Claude Code. It records how the project started, what
was decided, what exists, and what to do next. Read this once; `CLAUDE.md`
holds the standing rules.

## 1. Why this project exists

The owner wanted an open-source alternative to **CleanMyDev**
(cleanmydev.app), a closed-source Mac and Windows app by Dotsbit that finds
developer projects and moves rebuildable folders to the Trash. Its weak points
are trust and reach: closed source with broad disk access, an account needed to
download, telemetry and ad pixels, a leaderboard on by default, 5 free cleans a
day, no CLI and no Linux. AgentClean answers with: open source, no account,
nothing leaves the machine, every verdict explained, every clean reversible.

The full audit and gap list is in `docs/research/AUDIT.md`.

**Clean-room rule.** CleanMyDev was examined only from its public website,
the installed app's bundle metadata and embedded text, and its screens. Its
code was not decompiled, and none of its code, rule file, wording or artwork is
in this repository. Keep it that way.

## 2. Decisions (19 Nov 2026)

| Topic | Decision |
| --- | --- |
| Name | AgentClean. A trademark and GitHub-name check is still owed before launch |
| Base | Fork of [Zouuabi/CleanMyDev](https://github.com/Zouuabi/CleanMyDev) (MIT, Swift, unrelated to the commercial app despite the name) |
| Engine | Rust workspace in `engine/`; it is the only code allowed to delete (ADR 0001, 0002) |
| App | The inherited SwiftUI app, reduced and connected to the Rust engine in M3 |
| Scope | Developer only: projects, developer caches, Docker, simulators, disk map. Remove malware, uninstaller, fan and sensor control, speed test and system junk (ADR 0003) |
| Licence | Apache-2.0 for new work; inherited MIT and BSD-3-Clause notices kept (`NOTICE`, `LICENSES/`, `THIRD-PARTY-LICENSES.md`) |
| Game layer | Local stats and an original character in v1; no public leaderboard |
| Distribution | Developer ID, notarization, Homebrew cask |
| Platform | macOS 14 or later for the app (currently 26, lowered in M3); CLI on macOS and Linux |

## 3. Repository state

- Location on the owner's Mac: `/Users/muhammad/prod-project`.
- Private repository <https://github.com/Finjineersdev/AgentClean>, branch
  `main` (the local branch `agentclean-m0` tracks it), on top of 22 upstream
  commits. The upstream fork is the `upstream` remote. Make it public after
  the name check and the `SECURITY.md` rewrite.
- M0 to M3 are done; M4 (beta) is built and waits on the owner's signing keys
  and testers (`docs/milestones/M4.md`).
- Bundle identifier is `app.agentclean.AgentClean`, a placeholder until the
  owner names a domain they control.

## 4. What exists

### M0 (done)

- Upstream renamed to AgentClean throughout (folders, Xcode project, bundle
  ID, docs, workflows) by text replacement. **The app has not been built since
  the rename.**
- Upstream agent-hook configuration removed (`.claude/`, `.agents/`,
  `.entire/`; it ran a third-party CLI with telemetry on).
- Licence files, `NOTICE`, README.
- Rust workspace: rule catalogue with a linter, `deny` lists, the safety guard.
- Docs: `REQUIREMENTS.md`, `SAFETY-SPEC.md`, `RULE-SCHEMA.md`, ADRs 0001–0003.
- `.github/workflows/engine.yml`: fmt, clippy, tests, rules check on Ubuntu and
  macOS. It has never run, because the repository is not on GitHub yet.

### M1 (done)

- `quarantine.rs`: a clean is a run; folders move by rename into
  `<store>/runs/<run id>/items/NNNN`; the manifest is written before the first
  move and atomically after each one. Restore (whole run or one item) never
  overwrites and never recreates a deleted project. Purge deletes only inside
  the store and is the only thing that frees space.
- `size.rs`: allocated bytes, hard links counted once, symlinks not followed,
  no crossing volumes.
- CLI: `rules-check`, `plan`, `clean`, `history`, `restore`, `purge`.
  `clean` and `purge` are dry runs without `--execute`; Caution needs
  `--include-caution`.
- 40 tests: 35 in the engine (unit, property and filesystem tests), 5
  end-to-end tests of the binary.
- Rules: Node.js, Rust, Python (7 artifacts).

Default quarantine: `~/Library/Application Support/AgentClean/Quarantine`
(macOS), `$XDG_DATA_HOME/agentclean/quarantine` or
`~/.local/share/agentclean/quarantine` (Linux).

### M3 (done): the app on the engine

- The app runs the engine as a subprocess (ADR 0005). `CleanCore/Sources/AgentEngine`
  is the Swift client: argument lists, never a shell; it decodes every shape in
  `docs/CLI-JSON.md` and streams `scan`. Its tests decode the engine's own
  snapshot files and run the real binary.
- The engine binary is built by an Xcode build phase and embedded at
  `Contents/Helpers/agentclean` (not `Contents/MacOS`: the file system ignores
  case, and `agentclean` would replace the app's `AgentClean` executable).
- All inherited deletion code and out-of-scope modules are gone (malware,
  uninstaller, system junk, privacy, large files, disk map, fans, sensors,
  speed test, ports, the scheduler and the Full Disk Access gate). `CleanCore`
  keeps only the read-only Dev Stack inventory (about 850 lines).
  `scripts/check-swift-never-deletes.sh` fails CI if app code calls a file
  removal or move API.
- Screens: Overview (live scan, ready / Caution / active totals), Projects
  with per-folder selection (Safe pre-ticked in idle and dormant projects,
  Caution never, active projects need a per-project "clean anyway"), a Review
  sheet showing the engine's dry run before the real clean, Undo, History
  (restore a run or one item, purge with a confirmation), Dev Stack
  (read-only, removal commands to copy), Settings (scan folders, idle and
  dormant days, never-scan folders, pins), all through engine commands; the
  new `agentclean config` command writes settings.
- macOS 14 floor. Liquid Glass on macOS 26, a material fallback before it.
- Tests: `AgentEngineTests` (contract, end to end, selection rules for F14);
  `AgentCleanUITests` (XCUITest: scan, clean only what is ticked and undo,
  active-project consent, pinning, history restore and purge confirmation),
  run on a macOS 15 runner in CI (GitHub retired macOS 14 runners; the macOS 14 floor is checked by the deployment target only). Debug builds have a walkthrough
  (`AGENTCLEAN_QA=<folder>`) that drives every screen against throwaway
  projects, checks the results on disk and saves window snapshots; see
  `QA-CHECKLIST.md`.

### M4 (built): beta

See `docs/milestones/M4.md`. Global caches through each tool's own command
(ADR 0006, I20), a Caches screen, a menu bar item, accessibility labels and an
audit test, hardened runtime and notarization in the Release workflow, Sparkle
updates, and a Homebrew cask template. Signing, the appcast key and the tap
need the owner's accounts.

Verified on 2026-11-20: engine checks, CleanCore tests, the Release bundle
(universal, hardened runtime, no update key), the Debug walkthrough (22/22)
and all 7 UI tests on CI, including the accessibility audit on every screen.
The audit ignores only framework containers without identifiers, pop-up
menus' press action, and contrast findings the element's own pixels disprove
(it misreads text on the gradient); every ignored rule is in the test.

## 5. Known gaps and risks

| Gap | Where it gets fixed |
| --- | --- |
| Projects on network drives, and on drives without Unix permissions (exFAT, FAT), are skipped | ADR 0004 |
| GitHub Actions not pinned to commit SHAs | Before first release |
| Signing, notarization, Sparkle keys and the Homebrew tap need the owner's Apple and GitHub accounts | Owner, `docs/RELEASING.md` |
| UI tests need macOS automation mode; on a developer Mac run `sudo automationmodetool enable-automationmode-without-authentication` once (CI runners have it) | Owner's Mac |

## 6. Roadmap

| Milestone | Scope | Exit criteria |
| --- | --- | --- |
| M2 Discovery and catalogue | Project discovery, activity and idle verdicts, pins, rule schema 2 with fixtures, eleven ecosystems, JSON output, fuzzing and mutation tests | `docs/milestones/M2.md` |
| M3 App on the engine | UniFFI bridge or engine-as-subprocess, remove Swift deletion and out-of-scope modules, macOS 14 floor, per-folder selection, new `SECURITY.md` | No Swift code moves or deletes user files (CI check); app builds and passes UI tests on macOS 14 |
| M4 Beta | Menu bar, global caches via each tool's own command (toolchains listed separately, review-only), accessibility, signing, notarization, Sparkle updates, Homebrew cask | External beta with at least 20 testers, no open severity-one bugs |
| M5 v1.0 | Scheduled automatic clean (Safe only, size cap, undo), Docker and AI model stores (review-only), local stats and original character, user rules and per-repo config, docs site | Release checklist, security review |

## 7. Next session

1. Run the engine checks from `CLAUDE.md`, `cd CleanCore && swift test`, and
   `scripts/check-swift-never-deletes.sh`; all should pass.
2. Build the app and run the Debug walkthrough in `QA-CHECKLIST.md`.
3. If the owner has added the release secrets, tag `v0.4.0-beta.1` and check
   the release installs on a clean Mac. Otherwise start M5 (scheduled clean,
   Docker and AI model stores, review-only).

## 8. Facts worth keeping

From the owner's Mac (19 Nov 2026), useful as real-world test cases:

- Home-folder project directories include `dev`, `development`,
  `Mobile Projects`, `Laravel Projects`, `pythonprojects` and `websites`. The
  commercial app, scanning Documents, Desktop, Downloads and `websites`,
  reported a single project (`websites/abocare`, Node.js, about 681 MB of
  `node_modules` and `.next`). Discovery in M2 should find projects in all of
  these folders.
- Global caches present include `.gradle`, `.npm`, `.pub-cache`, `.cocoapods`,
  `.swiftpm`, `.cache`, `fvm`, and Android SDK folders. These belong to M4.

From the competitor's public behaviour (no code taken):

- Its engine is a Rust command-line tool behind a SwiftUI shell, the same
  split chosen here.
- Defaults: idle after 7 days, automatic rescan every 8 hours.
- Its "global caches" include installed software (Homebrew Cellar, nvm
  versions, global npm and pip packages). AgentClean must list those as
  toolchains, review-only, never as cache.

## 9. Reference

- Planning doc (private to the owner's claude.ai account):
  <https://claude.ai/artifact/J6D1sR1tKfARcC7cHiW6Gc>. Everything an agent needs
  from it is in this file and `docs/research/AUDIT.md`.
- Upstream: <https://github.com/Zouuabi/CleanMyDev>.
