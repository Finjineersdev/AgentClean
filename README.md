# AgentClean

A developer disk cleaner you can read the source of. It finds your projects,
shows which rebuildable folders are taking space (`node_modules`, `target`,
`.venv`, build output), explains why each one is safe to remove, and makes
every clean reversible. Local only: no account, no telemetry.

> **Status: pre-alpha (milestone M0).** The Rust engine can validate rules and
> tell you what it *would* allow in a project. It does not delete anything yet.
> The Mac app in this repository is the inherited upstream app, renamed; it has
> not been rebuilt or re-verified since the rename. Do not use it on data you
> care about.

AgentClean is a fork of [Zouuabi/CleanMyDev](https://github.com/Zouuabi/CleanMyDev)
(MIT). The upstream README is kept at [docs/UPSTREAM-README.md](docs/UPSTREAM-README.md).

## What is here

| Path | What | State |
| --- | --- | --- |
| `engine/` | Rust workspace: rule catalogue, safety guard, `agentclean` CLI | New; tested on Linux |
| `engine/rules/` | One TOML rule file per ecosystem | Node.js, Rust, Python so far |
| `AgentClean/`, `CleanCore/` | SwiftUI Mac app and its Swift engine, inherited | Renamed only |
| `docs/` | Requirements, safety specification, rule schema, decisions | Current |

## Try the engine

```bash
cd engine
cargo test
cargo run -p agentclean-cli -- rules-check rules
cargo run -p agentclean-cli -- plan --rules rules /path/to/a/project
```

`plan` prints each folder a rule matched and whether the safety guard would
allow it, with the reason for any refusal. It is read-only.

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

Before publishing, replace the `AGENTCLEAN_OWNER` placeholder with the GitHub
account or organisation that will host the project.
