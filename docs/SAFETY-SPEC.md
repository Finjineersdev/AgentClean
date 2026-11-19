# Safety specification

The one failure AgentClean cannot recover its reputation from is removing
something that cannot be rebuilt. This document states the invariants that
prevent it. Each invariant names the code that enforces it and the test that
proves it. A change that weakens an invariant needs an ADR.

Status: **I1–I12 are implemented and tested** in `engine/crates/agentclean-core`.
I13–I17 are specified for M1 and not implemented.

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
| I1 | A path is a candidate only if a linted rule names it and the rule's marker file is in the project root | `RuleFile::candidates` | (no candidate) |
| I2 | Rule paths are plain relative paths: no `.`, `..`, root, wildcard, expansion, or more than 4 components | `rules::lint_artifact_path` | lint error |
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

## Specified for M1

| # | Invariant |
| --- | --- |
| I13 | Cleaning moves to a quarantine folder on the same volume by rename; nothing is unlinked at clean time |
| I14 | Every run writes a manifest before the first move; a run restores whole or per item; restore never overwrites |
| I15 | A project with a running tool whose working directory is inside it, or that is pinned, is refused |
| I16 | Automatic cleaning takes Safe-class candidates only, from projects idle past the threshold, under a per-run size cap |
| I17 | No `sudo`, no privileged helper; external tools run with fixed argument lists |

## Test obligations

- Every invariant has at least one test that fails if the check is removed.
- `approval_implies_invariants` (property test) asserts I3, I4, I7 and I8 for
  arbitrary paths.
- Before M1 closes: fuzzing of the rule parser, and mutation testing of
  `guard.rs` and `deny.rs`.
