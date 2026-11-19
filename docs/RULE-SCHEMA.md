# Rule schema (version 1)

One TOML file per ecosystem in `engine/rules/`. Rules are written from each
tool's own documentation. A rule is a claim that a path is rebuildable; the
safety guard still decides whether a given directory may be cleaned.

```toml
schema = 1
id = "node"                 # lowercase letters, digits, '-'; unique
name = "Node.js"            # shown to the user

[detect]
any_files = ["package.json"]   # at least one must be a file in the project root

[[artifact]]
path = "node_modules"       # relative to the project root
class = "caution"           # "safe" or "caution"
reason = "Installed dependencies. Reinstall from the lockfile."
restore = "npm install (or pnpm install / yarn install)"
```

## Classes

| Class | Meaning | Pre-selected in the app | Automatic clean |
| --- | --- | --- | --- |
| `safe` | The next build recreates it with no user action | Yes | Allowed |
| `caution` | Needs a reinstall or download first | No | Never |

There is no "forbidden" class in rule files: protected names are built into
the engine and a rule that names one does not load.

## What the linter rejects

- Unknown fields, or a `schema` other than 1.
- An empty `detect.any_files`, or a marker that is not a plain file name.
- An artifact path that is empty, absolute, contains `.` or `..`, a backslash,
  a wildcard (`* ? [ ] { }`), `~` or `$`, ends in `/`, or has more than four
  components.
- An artifact path with a protected component (`.git`, `.env*`, keys,
  lockfiles) or whose last component is a source name (`src`, `lib`, `app`).
- A duplicate artifact path in one file, or a duplicate `id` across files.
- A missing `reason` or `restore`.

## Contributing a rule

1. Add or edit a file in `engine/rules/`.
2. Run `cargo run -p agentclean-cli -- rules-check rules` and `cargo test`.
3. In the pull request, link the tool's documentation that says the path is
   generated. Rule changes need two reviewers.

Planned for schema 2: fixture projects per rule (must-match and
must-not-match), tool-native cleanup commands, and stack-specific evidence for
generic names such as `build` and `dist`.
