# CLI JSON output

Every `opensweep` command takes `--json`. This page is the contract the Mac
app (M3) reads. Snapshots of each shape live in
`engine/crates/opensweep-cli/tests/snapshots/`, where temporary paths are
`$TMP` and sizes, times and run ids are masked.

## Rules

- Every object printed at the top level has `"schema_version": 1`. A change
  that removes or renames a field, or changes a field's meaning, raises it;
  adding a field does not. Readers must ignore fields they do not know.
- `scan` prints [JSON Lines](https://jsonlines.org/): one object per line, as
  projects are found. Every other command prints exactly one object.
- Paths are absolute strings, except item `path`, which is relative to the
  project `root`. Sizes are allocated bytes (`bytes`), as an integer.
- On failure the command prints `{"schema_version": 1, "error": "<message>"}`
  and exits non-zero. Exit code 3 means some items failed and the object
  describes which.
- Changing commands are still dry runs without `--execute`: `--json` changes
  only the output.

## Shared shapes

**Project** (`scan` lines, `plan`):

| Field | Type | Meaning |
| --- | --- | --- |
| `root` | string | Project folder |
| `verdict` | `"active"` \| `"idle"` \| `"dormant"` | Activity verdict |
| `pinned` | bool | Pinned projects are never cleaned |
| `reason` | string | One plain-English line behind the verdict |
| `items` | array of Item | Folders the rules offer |

**Item**:

| Field | Type | Meaning |
| --- | --- | --- |
| `path` | string | Relative to `root` |
| `rule` | string | Rule id (`node`, `rust`, …) |
| `class` | `"safe"` \| `"caution"` | Safe needs no user action to rebuild |
| `bytes` | integer or null | Size, when the guard allows the folder |
| `shared` | bool | Some files are hard links to, or a pnpm or Bun install cloned from, files elsewhere: `bytes` is an upper estimate |
| `refused` | string or null | Why the guard refuses it, when it does |

In `scan` and `plan`, activity does not refuse items: an active project's
items show sizes, and `verdict` says `clean` needs `--include-active`. Pinned
projects' items are refused.

**Run** (`clean`, `history`, `restore`, `purge`) is the quarantine manifest:
`run_id`, `format`, `created_unix`, and `entries`, each with `original`,
`project_root`, `stored`, `rule_id`, `class`, `bytes`, `state` (`pending`,
`moved`, `skipped`, `failed`, `restored`, `purged`), an optional `note`, and an
optional `volume`: the quarantine on another drive that holds the item.

## Commands

**`scan [ROOTS…] [--whole-disk] [--other-drives]`**: JSON Lines, in walk order, with every
project (the text `--all` filter does not apply). `--whole-disk` adds every
top folder of this computer's own disk where projects can live (on macOS the
folders on the data volume: `/Users`, `/Applications`, `/Library`, `/opt`,
`/usr/local`, `/private/tmp`; elsewhere `/`). `--other-drives` adds each
local, writable drive mounted under `/Volumes` (Linux: `/media`, `/run/media`,
`/mnt`), never a network drive, a read-only disk image or a Time Machine
backup; each is walked on its own volume:

```json
{"schema_version":1,"type":"project", …Project}
{"schema_version":1,"type":"skipped","path":"/x","reason":"unreadable","detail":"could not be read: …"}
{"schema_version":1,"type":"summary","projects":12,"ready":{"folders":3,"bytes":0},"caution":{…},"active":{…}}
```

`reason` for a skipped folder is one of `unreadable`, `protected` (refused by
macOS privacy protection; Full Disk Access fixes it), `other_volume`,
`depth_limit`, `entry_limit`, `excluded`, `installed` (installed software such
as Homebrew or `/Applications`, never walked or cleaned). The summary is always last: `ready`
is what `clean` takes with no flags, `caution` what `--include-caution` adds,
`active` what is held in active projects (`--include-active`).

**`plan PROJECT`**: `{"command":"plan","project": Project}`.

**`clean PROJECT…`**: `command`, `execute`, `projects` (Project with each item
carrying `action`: `"move"` or `"keep"`, `bytes` when moving, `reason` when
kept), `folders` and `bytes` moving, `held_caution`, and `run` (a Run after
`--execute`, otherwise null). `--only PATH` (repeatable, absolute) limits the
clean to those folders; a path the rules do not offer is an error and nothing
moves.

**`history`**: `{"command":"history","runs":[Run…]}`, newest first.

**`restore RUN`**: `{"command":"restore","run": Run}`.

**`purge RUN | --older-than-days N`**: `command`, `execute`, `bytes` held by
the selection, `freed`, and `runs` (after purging when `--execute`).

**`pin [FOLDER] [--days N | --remote]`**, **`unpin FOLDER`**: `{"command":"pin","pinned":[…],"until":{path: unix seconds},"remotes":["host/owner/repo"]}`. `config` also reports `pinned_until` and `pinned_remotes`; the
list after the change.

**`config [--idle-days N] [--dormant-days N] [--exclude DIR]… [--include DIR]…`**:
`command`, `path` (the config file), `idle_days`, `dormant_days`, `pinned`,
`exclude`, after any change. Like `pin`, it changes only OpenSweep's config.

**`caches`**: `caches`, one object per cache found on disk: `id`, `tool`,
`name`, `path`, `bytes`, `unreadable`, `command` (the tool's own command as
an array, or `null` when the cache is measured only), `installed` (the tool
was found), `note`.

**`clean-cache ID… [--execute]`**: `execute` and `caches`, the same objects;
with `--execute` each also has `status` (exit code, `null` if stopped),
`bytes_after`, `freed` and `stderr` (its last lines). A tool that could not
start has `status: null` and `error` instead, and the other caches still
run. Exit code 3 if any tool failed. Not undoable (ADR 0006).

**`rules-check DIR`**: `command`, `rule_files`, `artifacts`.
