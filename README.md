# AgentClean

A developer disk cleaner you can read the source of. It finds your projects,
shows which rebuildable folders are taking space (`node_modules`, `target`,
`.venv`, build output), explains why each one is safe to remove, and makes
every clean reversible. Local only: no account, no telemetry.

> **Status: pre-alpha (milestone M1).** The Rust engine and CLI can clean a
> project into a quarantine, restore it, and purge it to free the space. The
> rule catalogue covers Node.js, Rust and Python only, and there is no
> project discovery yet: you name each project folder.
> The Mac app in this repository is the inherited upstream app, renamed; it has
> not been rebuilt or re-verified since the rename. Do not use it on data you
> care about.

Contributors and coding agents: start with [CLAUDE.md](CLAUDE.md) and
[docs/HANDOFF.md](docs/HANDOFF.md).

AgentClean is a fork of [Zouuabi/CleanMyDev](https://github.com/Zouuabi/CleanMyDev)
(MIT). The upstream README is kept at [docs/UPSTREAM-README.md](docs/UPSTREAM-README.md).

## What is here

| Path | What | State |
| --- | --- | --- |
| `engine/` | Rust workspace: rule catalogue, safety guard, quarantine, `agentclean` CLI | New; tested on Linux |
| `engine/rules/` | One TOML rule file per ecosystem | Node.js, Rust, Python so far |
| `AgentClean/`, `CleanCore/` | SwiftUI Mac app and its Swift engine, inherited | Renamed only |
| `docs/` | Handoff, requirements, safety spec, rule schema, ADRs, research, milestone plans | Current |

## Try the engine

```bash
cd engine
cargo test
cargo build
B=target/debug/agentclean

$B plan  --rules rules ~/code/my-site            # what would be allowed, read-only
$B clean --rules rules ~/code/my-site            # dry run: lists what would move
$B clean --rules rules --execute ~/code/my-site  # move Safe folders into quarantine
$B clean --rules rules --execute --include-caution ~/code/my-site   # node_modules, .venv too
$B history                                       # runs, newest first
$B restore <run-id>                              # put everything back
$B purge <run-id> --execute                      # delete from quarantine, free the space
```

Cleaning moves folders into `~/Library/Application Support/AgentClean/Quarantine`
on macOS (`~/.local/share/agentclean/quarantine` on Linux). Because it is a
move on the same disk, **space is freed only when you purge**. Projects on
another volume are skipped in this version.

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
