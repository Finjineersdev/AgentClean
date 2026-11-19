# ADR 0005: The app runs the engine as a subprocess

Status: accepted, 20 Nov 2026. Replaces the UniFFI bridge planned in ADR 0001.

## Decision

The Mac app bundles the `agentclean` binary (`Contents/Helpers/agentclean`)
and drives it through `--json`, as documented in `docs/CLI-JSON.md`. The
Swift side is the `AgentEngine` target in `CleanCore/`: it starts the binary
with an argument list (never a shell), decodes each command's one JSON
document, and streams `scan`'s JSON Lines as projects are found. Cancelling a
task terminates the engine.

## Why

- The contract already exists and is pinned by snapshot tests in the engine;
  the Swift tests decode those same files, so drift on either side fails.
- No FFI: UniFFI generates `unsafe` glue and a C ABI, which the engine's
  lints forbid, and adds a code generator to the build.
- Isolation: an engine crash or hang cannot take the app down, and stopping a
  scan is terminating a process.
- The app and the CLI run exactly the same code path, so what users script is
  what the app does.

## Consequences

- The app build compiles the engine for arm64 and x86_64 and embeds the
  universal binary; it must be signed with the app (M4).
- Each command pays a process start (milliseconds) and re-reads the rules
  and config; fine for interactive use.
- Settings the app changes go through engine commands (`pin`, `unpin`,
  `config`), so the engine stays the only writer of its config.
