# AgentClean: notes for coding agents

AgentClean is an open-source, local-only developer disk cleaner for macOS (CLI
also on Linux). It finds rebuildable folders in developer projects
(`node_modules`, `target`, `.venv`, build output), explains why each is safe,
and makes every clean reversible.

**Start here:** read `docs/HANDOFF.md` for history, decisions, current state
and what to do next. The milestone plan in progress is `docs/milestones/M2.md`.

## Non-negotiable rules

1. **Only the Rust engine deletes or moves user files.** Never add file
   removal or move calls on user paths in Swift (`AgentClean/`, `CleanCore/`)
   or anywhere else. See `docs/adr/0002-single-deletion-path.md`.
2. **Every path goes rule → `Candidate` → `Guard::check` → `Approved` →
   `reverify` → move.** Do not add a constructor for `Candidate` or `Approved`
   outside `agentclean-core`, and do not weaken a check in `guard.rs`,
   `deny.rs`, `rules.rs` or `quarantine.rs` without an ADR in `docs/adr/`.
3. **A safety change ships with a test that fails without it.** The invariants
   and their tests are listed in `docs/SAFETY-SPEC.md`; keep that table current.
4. **Changing commands are dry runs unless `--execute`.** Caution-class folders
   also need `--include-caution`. Keep it that way in every new command.
5. **No network code** in the engine, and none in the app beyond the updater.
6. **Treat scanned repositories as hostile input.** Run git only with the
   hardened flags in `guard::check_untracked`; never build shell strings.
7. **Clean-room.** AgentClean competes with the closed-source app CleanMyDev
   (cleanmydev.app). Never copy its code, rule file, strings, artwork or
   copy, and never decompile it. Write rules from each tool's own docs.

## Build and test

Engine (Rust, needs `rustup`):

```bash
cd engine
cargo fmt --all --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo run -q -p agentclean-cli -- rules-check rules
```

All four must pass before you call engine work done. Lints forbid `unsafe`,
`unwrap`, `expect` and `panic` outside tests.

Try the CLI against a throwaway copy, never a real project, while developing:

```bash
cargo build && B=target/debug/agentclean
$B plan  --rules rules /tmp/some-copy
$B clean --rules rules --store /tmp/q /tmp/some-copy        # dry run
```

Inherited Mac app (Swift, untouched since the rename, not yet rebuilt):

```bash
cd CleanCore && swift test
xcodebuild -project AgentClean.xcodeproj -scheme AgentClean \
  -configuration Debug -derivedDataPath build/dd build       # macOS 26 SDK, Xcode 26+
```

## Layout

| Path | What |
| --- | --- |
| `engine/crates/agentclean-core/src/rules.rs` | Rule files: parse, lint, `Candidate` |
| `engine/crates/agentclean-core/src/deny.rs` | Names that are never deletable |
| `engine/crates/agentclean-core/src/guard.rs` | Safety guard, `Approved`, `reverify` |
| `engine/crates/agentclean-core/src/quarantine.rs` | Runs, manifest, restore, purge |
| `engine/crates/agentclean-core/src/size.rs` | Allocated-size measurement |
| `engine/crates/agentclean-cli/` | `agentclean` binary and end-to-end tests |
| `engine/rules/*.toml` | Rule catalogue, one file per ecosystem |
| `AgentClean/`, `CleanCore/` | Inherited SwiftUI app and Swift engine (to be reduced in M3) |
| `docs/` | Handoff, requirements, safety spec, rule schema, ADRs, research, milestones |

## Conventions

- Conventional Commits (`feat(engine): …`, `fix(guard): …`, `docs: …`).
- New code is Apache-2.0. Inherited files keep their MIT or BSD-3-Clause
  headers; never strip a notice.
- Plain English in UI and CLI text; every cleanable item carries a reason and
  a restore hint.
- Sizes in decimal units (KB, MB, GB), as Finder shows them.

## Inherited Swift app: things that bit upstream

- Never read the observable `AppModel` from `.commands {}` or a Scene-level
  binding setter without a change guard; it spins the main thread.
- A stable code signature is required; ad-hoc builds re-trigger Full Disk
  Access prompts that look like a hung scan.
- Docker sizes: use `UniqueSize` from `docker system df -v`.
- `URL.path(percentEncoded:)` for the home folder ends with `/`.
- Project discovery must walk below a project (monorepos) but skip bundles
  (`.xcodeproj`, `.app`), tool caches (`~/go/pkg`) and artifact folders.
