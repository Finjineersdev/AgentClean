# ADR 0006: Global caches are cleared by each tool's own command

Status: accepted, 20 Nov 2026. Implements F15.

## Decision

Caches shared by every project (npm, pnpm, Yarn, Bun, pip, uv, Go modules
and build cache, Deno, Composer, CocoaPods) are listed by `opensweep caches`
and cleared by `opensweep clean-cache <id> --execute`, which runs **that
tool's own command** with a fixed argument list (`npm cache clean --force`,
`go clean -modcache`, …). The engine never deletes cache files itself.

- No shell; stdin closed; working folder is the home folder, never a project
  (a project's `.npmrc` is hostile input, ADR rule 6); a 15-minute limit.
- The program is looked up on `PATH` plus the usual install folders, because
  an app started from Finder gets a bare `PATH`.
- Caches with no official command (Cargo registry, Gradle, Xcode
  DerivedData, Homebrew downloads) are measured and explained only.
  `brew cleanup` is not run: it also removes old versions of installed
  formulae, which are toolchains.
- Toolchains are never caches (I20): a cache path inside or above Homebrew's
  Cellar, global `node_modules`, nvm, rustup, pyenv, Volta, asdf, SDKMAN,
  mise, `~/.cargo/bin`, `~/go/bin` or the home folder is dropped, even when
  an environment variable points it there. Installed toolchains stay in the
  read-only Dev Stack.

## Why not quarantine

The tool owns the cache's layout and index; moving folders out from under
it can leave the tool's own bookkeeping broken, and a moved cache frees no
space until purged. Everything in these caches downloads again on demand.

## Consequences

- This is the one clean that cannot be undone. The CLI says so on every dry
  run, the app says so before the command runs, and caches are never
  selected for the user.
- `pnpm store prune` frees only unreferenced packages, so "freed" is measured
  after the run, not promised before it.
