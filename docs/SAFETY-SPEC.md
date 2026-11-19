# Safety specification

The one failure AgentClean cannot recover its reputation from is removing
something that cannot be rebuilt. This document states the invariants that
prevent it. Each invariant names the code that enforces it and the test that
proves it. A change that weakens an invariant needs an ADR.

Status: **I1–I14d and I18 are implemented and tested** in `engine/crates/agentclean-core`.
I15–I17 are specified and not implemented yet.

## Model

```
rule file ──lint──▶ RuleFile ──marker present──▶ Candidate ──Guard::check──▶ Approved ──reverify──▶ move
```

`Candidate` and `Approved` have private fields. Outside the engine crate there
is no way to construct either, so code that moves a path must have gone
through a linted rule and the guard. There is no free-form delete.

## Invariants in force

| # | Invariant | Enforced by | Refusal |
| --- | --- | --- | --- |
| I1 | A path is a candidate only if a linted rule names it, it exists, the rule's marker (file, folder or one-`*` name, never through a link) is directly in the project root, no `exclude_if` condition holds, and the artifact's `requires` evidence holds | `RuleFile::detects`, `RuleFile::candidates` | (no candidate) |
| I2 | Rule paths are plain relative paths: no `.`, `..`, root, wildcard, expansion, or more than 4 components | `rules::lint_artifact_path` | lint error |
| I2a | Rule markers are plain names and globs have exactly one `*` in a single component. Evidence paths obey I2 and never name a protected file. Evidence counts only as a regular file whose real path is the written path inside the project, read at most 1 MB; anything unreadable is no evidence | `rules::lint_name`, `lint_glob`, `lint_evidence`, `evidence_file` | lint error, or (no candidate) |
| I3 | The candidate, joined to the root, resolves strictly inside the root and is not the root | `Guard::check` | `OutsideProject`, `IsProjectRoot` |
| I4 | The target is a real directory: not a symlink, not a file | `Guard::check` | `Symlink`, `NotADirectory` |
| I5 | No symlink sits between the root and the target, even one pointing inside the project | `Guard::check` | `SymlinkInPath` |
| I6 | The project root is not `/`, a top-level folder, the home folder, or an ancestor of the home folder | `Guard::check` | `ProjectRootTooBroad` |
| I7 | No component is a VCS directory, credential store, env file, key or signing file, or lockfile | `deny::protected_component` | `Protected` |
| I8 | The final component is not a conventional source name (`src`, `lib`, `app`, `tests`, …) | `deny::protected_target` | `Protected` |
| I9 | The target is on the same volume as the project root | `Guard::check` | `CrossDevice` |
| I10 | The target does not contain its own repository | `Guard::check` | `ContainsRepository` |
| I11 | Git tracks nothing under the target; if the project is a repository and git cannot answer, refuse | `guard::check_untracked` | `ContainsTrackedFiles`, `VcsCheckFailed` |
| I12 | Immediately before a move, the directory is the same one that was approved (device, inode, creation time, canonical path) | `Approved::reverify` | `ChangedSinceApproval` |

### Notes on I11

The repository being inspected is untrusted input. A repository's own
configuration can name programs for git to run, so the guard calls git with
`core.fsmonitor=false` and `core.hooksPath=/dev/null`, clears `GIT_DIR` and
`GIT_WORK_TREE`, and never passes a path through a shell.
`git_check_does_not_run_repository_programs` proves the fsmonitor case.

### Known limits

- I12 relies on creation time to tell a recreated directory from the original
  when the inode number is reused. On a file system that records no creation
  time, a delete-and-recreate between check and move is not detected. The M1
  move is a rename into quarantine, so the cost of that race is a restorable
  quarantine entry, not data loss.
- Matching is by name. A user who keeps real work in a folder called
  `node_modules` outside version control is not protected by I7–I11. Quarantine
  and restore (I13, I14) are the backstop.
- Only git is checked for tracked files. Mercurial, Jujutsu and Subversion
  checkouts are refused only at the directory level (I7).

## Quarantine (M1)

| # | Invariant | Enforced by | Test |
| --- | --- | --- | --- |
| I13 | Cleaning moves each approved folder into the quarantine by rename on the same volume; nothing is copied or unlinked at clean time. A folder on another volume is skipped and recorded | `Store::clean` | `clean_then_restore_round_trips_exactly` |
| I14 | The manifest is written before the first move and rewritten atomically after each one; `reverify` runs immediately before each move | `Store::clean`, `write_manifest` | `changed_items_are_skipped_and_recorded`, `a_crash_after_a_move_is_recoverable` |
| I14a | Restore never overwrites and never recreates a missing project folder | `Store::restore` | `restore_never_overwrites`, `restore_refuses_when_the_project_is_gone` |
| I14b | Purge deletes only real directories at `items/<digits>` inside the store; run ids and item names from disk are validated; a malformed manifest stops the command | `Store::purge`, `parse_manifest` | `purge_deletes_only_quarantined_items`, `purge_removes_only_real_directories`, `tampered_manifests_are_refused`, `run_ids_cannot_escape_the_store` |
| I14d | Restore puts an item back only inside the project it came from: the manifest's original must be an absolute, normalized path strictly below its project root, and the parent folder's real path must still be the recorded one (no folder on the way replaced by a link) | `parse_manifest`, `Store::restore` | `manifests_cannot_send_items_outside_their_project`, `restore_does_not_follow_a_replaced_parent` |
| I14c | Every changing CLI command is a dry run without `--execute`; Caution folders also need `--include-caution` | `agentclean-cli` | `clean_is_a_dry_run_by_default`, `caution_folders_need_an_explicit_flag` |

## Discovery (M2)

Discovery only reads; what it finds still passes I1–I12 before anything moves.

| # | Invariant | Enforced by | Test |
| --- | --- | --- | --- |
| I18 | The project walk never follows a link, never enters another volume, the quarantine store, a bundle, a hidden folder, an artifact folder of a project it found, or a folder that belongs to its parent's project (Flutter's `android/`). A candidate is reported once, by the outermost project that claims it | `discover::discover` | `links_are_never_followed`, `other_volumes_are_not_entered`, `the_quarantine_store_is_not_entered`, `bundles_and_hidden_folders_are_not_entered`, `finds_every_project_in_a_layout_like_the_owners` |

Space is freed only when a run is purged, because a rename on the same volume
moves no data. The CLI says so after every clean.

Known limits: a crash between a rename and the manifest update leaves the entry
`pending`; restore and purge treat it as moved if its item is present. Restore
checks the destination is free and then renames, so a directory created in
that instant could, if empty, be replaced; a non-empty one makes the rename
fail safely. Two cleans running at the same moment use separate run folders
but are not otherwise coordinated.

## Specified, not implemented

| # | Invariant |
| --- | --- |
| I15 | A project with a running tool whose working directory is inside it, or that is pinned, is refused |
| I16 | Automatic cleaning takes Safe-class candidates only, from projects idle past the threshold, under a per-run size cap |
| I17 | No `sudo`, no privileged helper; external tools run with fixed argument lists |

## Test obligations

- Every rule has match and nomatch fixtures (`docs/RULE-SCHEMA.md`);
  `rule_fixtures_produce_the_expected_candidates` checks them.

- Every invariant has at least one test that fails if the check is removed.
- `approval_implies_invariants` (property test) asserts I3, I4, I7 and I8 for
  arbitrary paths.
- Fuzz targets in `engine/fuzz/`: `rules_parse` asserts that any accepted rule
  satisfies I2, I2a and I8; `manifest_load` asserts that any accepted manifest
  satisfies I14b and I14d and survives a write and re-read unchanged. Both run
  nightly for 10 minutes each (`engine-nightly` workflow).
- `cargo mutants` on `guard.rs`, `deny.rs`, `quarantine.rs`, `rules.rs` and `discover.rs` must report no
  surviving mutants; it runs nightly and the command is in `CONTRIBUTING.md`.
  Code that only one platform compiles is kept inside a shared function, so
  the tests that run cover the code that ships.
