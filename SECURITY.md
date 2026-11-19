# Security and safety

AgentClean moves folders out of your projects. This page says exactly what it
touches, what it needs, and how to report a problem.

## What it never does

- **Sends anything anywhere.** There is no network code in the engine and none
  in the app: no analytics, no crash reporter, no update check yet (an updater
  arrives with signed releases in M4 and will be the only network use).
- **Deletes at clean time.** Cleaning *moves* each folder into a quarantine on
  the same drive, by rename. Space comes back only when you purge a run, and
  purge deletes only inside the quarantine.
- **Lets the app touch files itself.** Only the Rust engine moves or deletes
  anything (ADR 0002). The app runs the bundled engine binary and reads its
  JSON output (ADR 0005); a CI check fails the build if app code calls a file
  removal or move API.
- **Cleans pinned projects**, or anything outside a folder a rule names. The
  safety guard refuses system folders, your home folder itself, symbolic
  links, folders tracked by git, and anything that changed between the check
  and the move. The invariants and their tests are listed in
  [docs/SAFETY-SPEC.md](docs/SAFETY-SPEC.md).
- **Runs programs from your repositories.** Git is run only with hooks,
  fsmonitor, pagers, signature programs and remote fetches turned off, and
  never through a shell.

## Defaults that protect you

- Every changing CLI command is a dry run unless you pass `--execute`. In the
  app, every clean shows the engine's dry run first and moves only the folders
  you ticked.
- **Caution** folders (`node_modules`, `.venv`, …), which need a reinstall, are
  never ticked for you.
- **Active** projects (recent edits or commits, or a tool running inside) are
  left alone unless you say so for that project.
- On another drive, the quarantine is a private folder at the top of that
  drive; network drives are skipped (ADR 0004).

## What it needs

| Access | Why |
| --- | --- |
| Read access to the folders you scan | To find projects and measure folders. Full Disk Access is **not** required; folders macOS will not show are listed as skipped. |
| Write access to your projects | To move cleanable folders into the quarantine, and back on restore. |
| `~/Library/Application Support/AgentClean` | Quarantine, run history and settings (`config.toml`). |

The Dev Stack screen reads what tools are installed (Homebrew, version
managers, Docker through your `docker` CLI) and shows each tool's own removal
command for you to copy. It never runs those commands.

## Reporting

If AgentClean moved or deleted something it should not have, or you found a
way to make it do so, use GitHub's private vulnerability reporting on this
repository. Include the output of `agentclean history --json` for the run
involved if you can.
