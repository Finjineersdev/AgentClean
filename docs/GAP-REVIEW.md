# Gap review (20 Nov 2026)

What AgentClean set out to be (`HANDOFF.md` §1, `REQUIREMENTS.md`,
`research/AUDIT.md`), checked against the code as of commit `d862b08`.
Each gap names where the evidence is. Ordered by what hurts a real user most.

## Fixed during this review

| Gap | Fix |
| --- | --- |
| The scan covered only the home folder; macOS asked separately for each protected folder | `scan --whole-disk`: every top folder of the data volume, one walk each (I18 holds). The app scans the whole Mac by default and asks once for Full Disk Access |
| **Safety:** a whole-disk scan offered Homebrew's own `npm` and `firebase-tools` `node_modules` (275 MB) for cleaning. The guard had no idea of installed software; anyone adding `/opt` as a scan folder was exposed | I21: the guard refuses any project inside installed software (`node_modules`, `site-packages`, `Cellar`, Homebrew, MacPorts, Nix, `/Applications`, `/Library`, `/usr`, home toolchains). The walk reports those folders as skipped instead of entering them |
| iOS archives and simulators were not measured | Caches lists Xcode archives, simulators, simulator caches, preview simulators and Xcode's cache, measured only |

## 1. Product gaps (the idea is not delivered without these)

| # | Gap | Evidence | Proposal |
| --- | --- | --- | --- |
| P1 | **Cleaning is one project at a time.** This Mac has 709 projects. There is no "clean everything that is ready" | No cross-project path in `AppModel` or `ProjectsView`; `clean` takes one project | Engine: `clean` accepts several projects in one run, so Undo is one action. App: "Review all ready" from Home and from each language card |
| P2 | **Cleaning frees no space until a purge, and nothing purges automatically.** The quarantine grows forever, so "11.2 MB can go" never reaches the disk unless the user finds Purge | `purge --older-than-days` exists in the engine (`quarantine.rs:405`); the app never calls it | Setting "Free quarantined space after 7 days" (default on), run on launch through the engine. Say "frees on <date>" after every clean, and show "held" on Home |
| P3 | **Scan results are not kept.** Every launch starts empty and needs a 40-second scan | No persistence in `AppModel`; menu bar totals live in memory | Save the last scan's JSON in Application Support, show it at once with "scanned 2 h ago", rescan in the background on open (setting), like the competitor's scan on open and 8-hour rescan |
| P4 | **Other drives are not scanned.** Developers keep projects on external SSDs | `whole_disk_roots` skips `/Volumes` by design | Settings: "Also scan other drives", listing local drives only (never network or Time Machine); each walked on its own volume |
| P5 | Scheduled automatic clean (F17, M5) | Not started | Safe class only, idle and dormant projects only, size cap, notification with Undo; needs open at login (`SMAppService`) and notifications, neither exists yet |
| P6 | Local stats and milestones (F19) | History holds every run; nothing adds them up | Home: freed this month and all time, by language; milestone moments for the character. No network |
| P7 | The character is not original | Characters are Open Peeps (CC0 stock); the requirement says "an original character" | Commission or draw an AgentClean mascot before launch; keep the Peeps as placeholders |
| P8 | User rules and a per-repository config (F20) | `rules::load_dir` reads the bundled catalogue only | `~/.config/agentclean/rules/*.toml` through the same linter; `.agentclean.toml` in a repo for pin, exclude and extra artifacts |
| P9 | Pins are a path and forever | `activity.rs` has no expiry or git identity; the upstream had "pin until a date" and pins that follow the git remote | Pin until a date; key pins by git remote so they survive moves and renames |

## 2. Coverage gaps (things that fill a developer's disk and are not shown)

| # | Gap | Proposal |
| --- | --- | --- |
| C1 | Docker: Dev Stack reads it (`DockerService`), but there is no review-only Docker category with unique image sizes (F18) | Caches section "Docker", measured with `docker system df -v` `UniqueSize`; clearing through Docker's own commands under ADR 0006 |
| C2 | iOS DeviceSupport is a toolchain (I20), so it is never measured as a cache, only listed in Dev Stack above 1 MB | ADR 0007: measure it (not clear it) in Caches, with the versions present; same for watchOS and tvOS DeviceSupport |
| C3 | Simulators are measured; the one safe command, `xcrun simctl delete unavailable`, is only mentioned | Offer it as the tool's own command (ADR 0006 path, never pre-selected) |
| C4 | Editor and AI tool caches (VS Code, Cursor, JetBrains, Playwright and Cypress browsers, Hugging Face and Torch weights, Ollama and LM Studio models) | Review-only caches; model stores never pre-selected (F18) |
| C5 | The disk map: ADR 0003 says it stays, M3 removed it | Bring back a read-only treemap of language → project → folder on Home, or amend ADR 0003 |

## 3. UX gaps

| # | Gap | Proposal |
| --- | --- | --- |
| U1 | Projects lists all 709 projects; most have nothing to clean. Search only, no filters | Default filter "Has something to clean"; filters for verdict and language; sort by size, age, name |
| U2 | Language cards open a list but cannot act on it | "Review all <language>" in the drawer (needs P1) |
| U3 | No first-run guide: quarantine, whole-Mac scan and Full Disk Access are explained only in passing | Three-step onboarding with the host character; Full Disk Access step with a live check |
| U4 | Caches shows full `/Users/...` paths and "0 selected · 0 bytes"; measured-only rows mention commands with no copy button | Paths with `~` and middle truncation; sentence without middle dots; copy button as in Dev Stack |
| U5 | Xcode folders appear in both Dev Stack and Caches | Keep sizes in Caches; Dev Stack links to them |
| U6 | History has per-run actions only | "Free everything older than…", totals held and freed |
| U7 | Keyboard: only ⌘R | ⌘F search, arrow keys through projects, Space to tick, ⌘⏎ Review |
| U8 | Dark mode only (`preferredColorScheme(.dark)`) | A light palette for the same tokens |

## 4. Verification and release gaps

| # | Gap | Proposal |
| --- | --- | --- |
| R1 | **CI has not run since the billing block.** The UI tests and accessibility audit have not seen the two redesigns, the whole-disk mode or the access banner | Fix billing, run CI with UI tests (`gh workflow run CI`) |
| R2 | No UI test for language cards and drawer, the whole-Mac toggle, the access banner | Add them to `AgentCleanUITests` |
| R3 | App icon is still the upstream's; bundle ID is a placeholder; trademark check owed | Owner (`REQUIREMENTS.md` open items) |
| R4 | Actions not pinned to SHAs; no SBOM or build attestation (N7) | Pin actions; `actions/attest-build-provenance` and an SBOM in `release.yml` |
| R5 | Signing, notarization, Sparkle keys, Homebrew tap; 20-tester beta; five-developer usability pass (F13) | Owner, then the beta |
| R6 | The walk is single-threaded: a whole-disk scan takes about 43 s in a debug build (first result after 0.4 s, so N4 holds) | Measure a release build; parallelise per root if it is slow |

## Suggested order

1. R1 (CI), P2 (auto-free), P3 (keep results), P1 (bulk clean), U1 (filters):
   the everyday loop of scan, review, clean, space freed.
2. P4 (other drives), C1 to C4 (coverage), U3 (onboarding), U4 to U7.
3. M5 proper: P5 (scheduled clean), P6 (stats), P8 (user rules), P9 (pins),
   C5 (disk map), P7 (original character), R3 to R5 (release).
