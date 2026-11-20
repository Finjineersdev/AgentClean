<img src="docs/assets/icon.png" width="128" alt="AgentClean icon">

# AgentClean

A developer disk cleaner you can read the source of. It finds your projects,
shows which rebuildable folders are taking space (`node_modules`, `target`,
`.venv`, build output), explains why each one is safe to remove, and makes
every clean reversible. Local only: no account, no telemetry.

> **Status: beta (milestone M4).** The Rust engine and CLI find projects,
> judge whether each is in use, and clean rebuildable folders into a
> quarantine you can restore from. The Mac app (macOS 14 or later) runs that
> same engine: scan, pick folders, review the dry run, clean, undo; clear
> global tool caches with each tool's own command; a menu bar item.

## Install

```bash
brew install --cask finjineersdev/tap/agentclean
```

or download the `.dmg` from [Releases](https://github.com/Finjineersdev/AgentClean/releases)
and drag AgentClean to Applications. macOS 14 or later.

AgentClean is open source and not notarized by Apple, so:

- **First launch:** macOS blocks it. Open System Settings > Privacy & Security
  and click **Open Anyway** next to AgentClean.
- **After an update:** macOS may forget Full Disk Access. If a scan stops
  seeing folders, turn AgentClean off and on again under Privacy & Security >
  Full Disk Access.

Contributors and coding agents: start with [CLAUDE.md](CLAUDE.md) and
[docs/HANDOFF.md](docs/HANDOFF.md).

AgentClean is a fork of [Zouuabi/CleanMyDev](https://github.com/Zouuabi/CleanMyDev)
(MIT). The upstream README is kept at [docs/UPSTREAM-README.md](docs/UPSTREAM-README.md).

## What is here

| Path | What | State |
| --- | --- | --- |
| `engine/` | Rust workspace: rule catalogue, safety guard, quarantine, `agentclean` CLI | New; tested on Linux |
| `engine/rules/` | One TOML rule file per ecosystem | Eleven ecosystems |
| `AgentClean/` | SwiftUI Mac app; runs the bundled engine (ADR 0005) | Alpha; macOS 14+ |
| `CleanCore/` | `AgentEngine` (Swift client for the engine) and the read-only Dev Stack inventory | Tested |
| `docs/` | Handoff, requirements, safety spec, rule schema, ADRs, research, milestone plans | Current |

## Try the engine

```bash
cd engine
cargo test
cargo build
B=target/debug/agentclean

$B scan                                          # every project in your home folder, read-only
$B scan ~/code                                   # or just these folders
$B plan  ~/code/my-site                          # one project: what would be allowed
$B clean ~/code/my-site                          # dry run: lists what would move
$B clean --execute ~/code/my-site                # move Safe folders into quarantine
$B clean --execute --include-caution ~/code/my-site   # node_modules, .venv too
$B pin ~/code/client-app                          # never clean this folder
$B history                                       # runs, newest first
$B restore <run-id>                              # put everything back
$B purge <run-id> --execute                      # delete from quarantine, free the space
```

Cleaning moves folders into `~/Library/Application Support/AgentClean/Quarantine`
on macOS (`~/.local/share/agentclean/quarantine` on Linux). Because it is a
move on the same disk, **space is freed only when you purge**. Projects on
another drive go to a hidden `.agentclean-quarantine` folder at the top of
that drive; projects on network drives are skipped.

Add `--json` to any command for machine-readable output
([`docs/CLI-JSON.md`](docs/CLI-JSON.md)). The rule catalogue is built in; `--rules DIR` uses a folder of rule files
instead. Each project gets a verdict first. **Active** projects (edited or committed in
the last 14 days, or with a tool running inside) are left alone unless you
pass `--include-active`; **pinned** projects are never cleaned.

## Design in three rules

1. **One deletion path.** Only the Rust engine may move or delete, and only a
   path that a rule matched and the safety guard approved. See
   [docs/SAFETY-SPEC.md](docs/SAFETY-SPEC.md).
2. **Rules are data.** Ecosystems are declared in linted, tested TOML files.
   See [docs/RULE-SCHEMA.md](docs/RULE-SCHEMA.md).
3. **Refuse when unsure.** Tracked files, symlinks, unreadable repositories and
   anything that changed since it was checked are all refusals.

Scope, milestones and non-goals are in [docs/REQUIREMENTS.md](docs/REQUIREMENTS.md).

## Licence

New work is under the Apache License 2.0 ([LICENSE](LICENSE)). Code inherited
from the upstream project stays under its MIT licence, and code it adapted
from other projects keeps theirs; see [NOTICE](NOTICE) and
[THIRD-PARTY-LICENSES.md](THIRD-PARTY-LICENSES.md).
