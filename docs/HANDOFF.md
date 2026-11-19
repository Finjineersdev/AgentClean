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
- M0, M1 and M2.1 are committed.
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

### Inherited and untouched

- `AgentClean/` (SwiftUI app) and `CleanCore/` (Swift engine, about 8,900
  lines with the app). `CleanCore` still has its own deletion code; that is
  allowed only until M3.
- `SECURITY.md` still describes upstream behaviour (Full Disk Access required,
  fan control, speed test). **It is inaccurate for AgentClean** and must be
  rewritten in M3; until then the README status note warns users.
- `.github/workflows/ci.yml` and `release.yml` are upstream's (macOS 26
  runners, Swift tests, cask bump), renamed only.
- `QA-CHECKLIST.md`, `docs/RELEASING.md`: upstream, renamed only.

## 5. Known gaps and risks

| Gap | Where it gets fixed |
| --- | --- |
| No project discovery; the CLI needs explicit project paths | M2 |
| Only three ecosystems | M2 |
| No idle or active verdict, no pins | M2 |
| No JSON output for the app to consume | M2 |
| Fuzzing and mutation testing owed | M2 (first tasks) |
| Projects on a different volume from the quarantine are skipped, not cleaned | M2 decision, see `docs/milestones/M2.md` |
| Generic names (`build`, `dist`, `bin`, `obj`) need stack-specific evidence before rules can use them | M2, rule schema 2 |
| Mac app unbuilt since the rename; macOS 26 floor; Swift deletion path still present | M3 |
| `SECURITY.md` inaccurate | M3 |
| GitHub Actions not pinned to commit SHAs | Before first release |
| No signing, notarization or updater configured for AgentClean | M4 |

## 6. Roadmap

| Milestone | Scope | Exit criteria |
| --- | --- | --- |
| M2 Discovery and catalogue | Project discovery, activity and idle verdicts, pins, rule schema 2 with fixtures, eleven ecosystems, JSON output, fuzzing and mutation tests | `docs/milestones/M2.md` |
| M3 App on the engine | UniFFI bridge or engine-as-subprocess, remove Swift deletion and out-of-scope modules, macOS 14 floor, per-folder selection, new `SECURITY.md` | No Swift code moves or deletes user files (CI check); app builds and passes UI tests on macOS 14 |
| M4 Beta | Menu bar, global caches via each tool's own command (toolchains listed separately, review-only), accessibility, signing, notarization, Sparkle updates, Homebrew cask | External beta with at least 20 testers, no open severity-one bugs |
| M5 v1.0 | Scheduled automatic clean (Safe only, size cap, undo), Docker and AI model stores (review-only), local stats and original character, user rules and per-repo config, docs site | Release checklist, security review |

## 7. First session in Claude Code

1. Install Rust if needed: `curl https://sh.rustup.rs -sSf | sh` (or
   `brew install rustup` then `rustup-init`).
2. Run the four engine checks from `CLAUDE.md`; all should pass.
3. Commit the existing work, for example:
   - `chore: fork CleanMyDev as AgentClean, add Rust engine skeleton (M0)`
     (the staged changes)
   - `feat(engine): quarantine, restore, purge and CLI (M1)`
   - `docs: handoff, research and M2 plan`
4. Decide the GitHub owner, replace `AGENTCLEAN_OWNER`, push, and confirm the
   `engine` workflow goes green on both runners.
5. Optional but useful early: open `AgentClean.xcodeproj` in Xcode 26 and
   build once, to confirm the rename did not break the project.
6. Start M2 with `docs/milestones/M2.md`, task M2.1.

A first prompt that works well:

> Read CLAUDE.md, docs/HANDOFF.md and docs/milestones/M2.md. Run the engine
> checks and report the results. Then propose how you will implement task M2.1
> before writing code.

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
