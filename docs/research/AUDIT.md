# Competitive audit (19 Nov 2026)

Summary of the research behind AgentClean. Facts about CleanMyDev come from
its public website, release notes, privacy policy and terms, from the
installed app's bundle metadata and embedded text, and from using its screens
without cleaning anything. No code was decompiled or copied.

## CleanMyDev (cleanmydev.app), version 0.1.9

| Item | Finding |
| --- | --- |
| Maker | Dotsbit (from the app bundle); not named on the website |
| Platforms | macOS 14+, Windows 10+ |
| First release | 25 Oct 2026 (macOS), 4 Nov 2026 (Windows) |
| Price | Free with 5 project cleans a day; Pro $3 a month, $30 a year, $100 lifetime |
| Pro only | Unlimited cleans, automatic clean, open at login, end-of-day review |
| Architecture | SwiftUI app driving a separate Rust command-line engine; SQLite state; one YAML rule file; Sparkle updates |
| Stacks | Node and React Native, Flutter, iOS and Xcode, Android, Rust, Python, PHP, Go |
| Classes | Safe, Caution, Forbidden |
| Idle | Newest edit in a few source-folder names, plus last git commit, against an idle threshold (default 7 days) |
| Active | A process with one of about a dozen tool names is running |
| Cleaning | Dry run unless executed; Caution needs a second flag; pinned and active projects refused; moves to the system Trash |
| Network | Account (Firebase), daily usage ping, numeric events, PostHog, Reddit and X pixels, tracked email, public leaderboard on by default after sign-in |
| Game layer | Character that grows per GB reclaimed, accessories at milestones, share cards, public leaderboard |

### Screens

Dashboard (disk, reclaimable total, stack "tree" animation), Projects (cards
with search, idle filter, sort), project detail sheet (folder list with Safe
or Caution and one Clean button for all), Global Caches (measure only), Activity
(history), Leaderboard (embedded web page). Settings: idle threshold, scan on
open, rescan interval, scan roots, character, privacy.

## Gaps AgentClean addresses

| # | Gap in CleanMyDev | AgentClean answer |
| --- | --- | --- |
| 1 | Closed source with broad disk access | Open source, reproducible builds |
| 2 | Account to download, telemetry, ad pixels | No account, no telemetry; update check only |
| 3 | Leaderboard on by default | Stats local only |
| 4 | Paywalled cleans and automation | Everything free |
| 5 | No CLI, no Linux | CLI with dry run and JSON, macOS and Linux |
| 6 | Trash is the only undo; space not freed until Trash is emptied | Quarantine with manifest, restore by run, honest "freed" figures |
| 7 | Idle verdict not explained | Reasons on every verdict |
| 8 | One Clean button per project for Safe and Caution together | Per-folder selection, Caution never pre-selected |
| 9 | Installed software (Homebrew Cellar, nvm, global packages) listed as "cache" | Toolchains listed separately, review-only |
| 10 | Thin secret protection (about a dozen names, no `.env.*` wildcard) | Pattern deny-list plus a git tracked-files check |
| 11 | Generic names (`build`, `dist`, `vendor`) matched by name only | Tracked-files check now; stack evidence in rule schema 2 |
| 12 | Narrow scan roots found 1 project on a machine with many | Whole-home discovery with skipped and unreadable folders reported |
| 13 | Go detected but nothing cleanable | Go handled through `go clean` (M4) |
| 14 | Process check by name only | Processes matched by working directory |
| 15 | Single rule file, no per-rule tests | One file per ecosystem, fixtures per rule |
| 16 | No Docker, AI model stores or editor caches | Review-only categories (M5) |

## What CleanMyDev does well (match it)

Safe, Caution and Forbidden classes; a confirmation step listing every folder
with size and class; skipping active projects; pinning; a menu bar summary; a
dashboard that makes reclaimable space obvious; Reduce Motion support.

## Other tools

| Tool | Type | Notes |
| --- | --- | --- |
| Zouuabi/CleanMyDev | SwiftUI Mac app, MIT | AgentClean's upstream; quarantine by default; Active, Idle, Dormant with reasons |
| cleanmydev.com | Mac app, $9.99 once | Unrelated product with the same name; cache-first |
| kondo | Rust CLI and GUI | Many project types; deletes permanently |
| npkill | Node CLI | `node_modules` focus; deletes permanently |
| CleanDev (cleandev.cc) | Mac menu bar app | Free, closed source, cache-first |
| Dev Disk Hygiene | Mac App Store | Sandboxed, Xcode-centred |
