//! Quarantine round trips against real directories.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use crate::guard::{Approved, Guard};
use crate::quarantine::{Error, Manifest, State, Store, MANIFEST};
use crate::rules::Candidate;

struct Fixture {
    _tmp: TempDir,
    base: PathBuf,
    root: PathBuf,
    guard: Guard,
    store: Store,
}

impl Fixture {
    fn approve(&self, rel: &str) -> Approved {
        self.guard
            .check(&self.root, &Candidate::unchecked(rel))
            .unwrap()
    }

    fn items(&self, run: &Manifest) -> PathBuf {
        self.store
            .root()
            .join("runs")
            .join(&run.run_id)
            .join("items")
    }
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(tmp.path()).unwrap();
    let root = base.join("work/proj");
    fs::create_dir_all(root.join("node_modules/a/b")).unwrap();
    fs::create_dir_all(root.join("target/debug")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("node_modules/a/index.js"),
        "module.exports = 1;\n",
    )
    .unwrap();
    fs::write(root.join("node_modules/a/b/data.bin"), vec![42u8; 70_000]).unwrap();
    fs::write(root.join("target/debug/app"), vec![1u8; 9_000]).unwrap();
    fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let exe = root.join("node_modules/a/run.sh");
        fs::write(&exe, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
        std::os::unix::fs::symlink("index.js", root.join("node_modules/a/alias.js")).unwrap();
    }
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let store = Store::open(&base.join("store")).unwrap();
    Fixture {
        guard: Guard::new(Some(home)),
        _tmp: tmp,
        base,
        root,
        store,
    }
}

/// Everything about a tree that a restore must preserve.
fn snapshot(dir: &Path) -> BTreeMap<PathBuf, String> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(p) = stack.pop() {
        let meta = fs::symlink_metadata(&p).unwrap();
        let rel = p.strip_prefix(dir).unwrap().to_path_buf();
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            meta.permissions().mode()
        };
        #[cfg(not(unix))]
        let mode = 0;
        let desc = if meta.file_type().is_symlink() {
            format!("link -> {}", fs::read_link(&p).unwrap().display())
        } else if meta.is_dir() {
            for c in fs::read_dir(&p).unwrap() {
                stack.push(c.unwrap().path());
            }
            format!("dir {mode:o}")
        } else {
            format!("file {mode:o} {:?}", fs::read(&p).unwrap())
        };
        out.insert(rel, desc);
    }
    out
}

fn read_manifest(f: &Fixture, run: &Manifest) -> Manifest {
    let path = f.store.root().join("runs").join(&run.run_id).join(MANIFEST);
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn clean_then_restore_round_trips_exactly() {
    let f = fixture();
    let before_nm = snapshot(&f.root.join("node_modules"));
    let before_target = snapshot(&f.root.join("target"));

    let run = f
        .store
        .clean(vec![f.approve("node_modules"), f.approve("target")])
        .unwrap();
    assert!(
        run.entries.iter().all(|e| e.state == State::Moved),
        "{run:#?}"
    );
    assert!(!f.root.join("node_modules").exists() && !f.root.join("target").exists());
    assert!(f.root.join("src/main.rs").exists());
    assert!(run.bytes_held() >= 79_000);
    assert_eq!(read_manifest(&f, &run), run);
    assert_eq!(snapshot(&f.items(&run).join("0000")), before_nm);

    let back = f.store.restore(&run.run_id, None).unwrap();
    assert!(
        back.entries.iter().all(|e| e.state == State::Restored),
        "{back:#?}"
    );
    assert_eq!(snapshot(&f.root.join("node_modules")), before_nm);
    assert_eq!(snapshot(&f.root.join("target")), before_target);
    assert_eq!(back.bytes_held(), 0);
}

#[test]
fn restore_one_item() {
    let f = fixture();
    let run = f
        .store
        .clean(vec![f.approve("node_modules"), f.approve("target")])
        .unwrap();
    let back = f.store.restore(&run.run_id, Some(1)).unwrap();
    assert_eq!(back.entries[0].state, State::Moved);
    assert_eq!(back.entries[1].state, State::Restored);
    assert!(f.root.join("target/debug/app").exists());
    assert!(!f.root.join("node_modules").exists());
    assert!(matches!(
        f.store.restore(&run.run_id, Some(9)),
        Err(Error::UnknownItem(..))
    ));
}

#[test]
fn restore_never_overwrites() {
    let f = fixture();
    let run = f.store.clean(vec![f.approve("node_modules")]).unwrap();
    fs::create_dir_all(f.root.join("node_modules")).unwrap();
    fs::write(f.root.join("node_modules/new.txt"), "reinstalled").unwrap();

    let back = f.store.restore(&run.run_id, None).unwrap();
    assert_eq!(back.entries[0].state, State::Moved);
    assert!(back.entries[0].note.is_some());
    assert_eq!(
        fs::read_to_string(f.root.join("node_modules/new.txt")).unwrap(),
        "reinstalled"
    );
    assert!(f.items(&run).join("0000/a/index.js").exists());
}

#[test]
fn restore_refuses_when_the_project_is_gone() {
    let f = fixture();
    let run = f.store.clean(vec![f.approve("target")]).unwrap();
    fs::remove_dir_all(&f.root).unwrap();
    let back = f.store.restore(&run.run_id, None).unwrap();
    assert_eq!(back.entries[0].state, State::Moved);
    assert!(
        !f.root.exists(),
        "restore must not recreate a deleted project"
    );
}

#[test]
fn changed_items_are_skipped_and_recorded() {
    let f = fixture();
    let nm = f.approve("node_modules");
    let target = f.approve("target");
    fs::remove_dir_all(f.root.join("target")).unwrap();
    fs::create_dir_all(f.root.join("target")).unwrap();

    let run = f.store.clean(vec![nm, target]).unwrap();
    assert_eq!(run.entries[0].state, State::Moved);
    assert_eq!(run.entries[1].state, State::Skipped);
    assert!(run.entries[1].note.as_deref().unwrap().contains("changed"));
    assert!(f.root.join("target").exists());
    assert_eq!(read_manifest(&f, &run), run);
}

#[test]
fn a_crash_after_a_move_is_recoverable() {
    let f = fixture();
    let run = f.store.clean(vec![f.approve("node_modules")]).unwrap();
    // Simulate a crash between the rename and the manifest update.
    let mut stale = run.clone();
    stale.entries[0].state = State::Pending;
    let path = f.store.root().join("runs").join(&run.run_id).join(MANIFEST);
    fs::write(&path, serde_json::to_vec(&stale).unwrap()).unwrap();

    let back = f.store.restore(&run.run_id, None).unwrap();
    assert_eq!(back.entries[0].state, State::Restored);
    assert!(f.root.join("node_modules/a/index.js").exists());
}

#[test]
fn purge_deletes_only_quarantined_items() {
    let f = fixture();
    let run = f.store.clean(vec![f.approve("node_modules")]).unwrap();
    let decoy = f.base.join("decoy");
    fs::create_dir_all(&decoy).unwrap();
    fs::write(decoy.join("keep"), "x").unwrap();

    let purged = f.store.purge(&run.run_id).unwrap();
    assert_eq!(purged.entries[0].state, State::Purged);
    assert_eq!(purged.bytes_held(), 0);
    assert_eq!(purged.bytes_cleaned(), run.bytes_held());
    assert!(!f.items(&run).join("0000").exists());
    assert!(decoy.join("keep").exists());
    assert!(f.root.join("src/main.rs").exists());

    // Purged items cannot be restored, and a second purge is a no-op.
    let again = f.store.restore(&run.run_id, None).unwrap();
    assert_eq!(again.entries[0].state, State::Purged);
    assert_eq!(f.store.purge(&run.run_id).unwrap(), again);
}

#[test]
fn tampered_manifests_are_refused() {
    let f = fixture();
    let run = f.store.clean(vec![f.approve("target")]).unwrap();
    let path = f.store.root().join("runs").join(&run.run_id).join(MANIFEST);
    let mut evil = run.clone();
    evil.entries[0].stored = "../../../work/proj/src".to_owned();
    fs::write(&path, serde_json::to_vec(&evil).unwrap()).unwrap();

    assert!(matches!(
        f.store.purge(&run.run_id),
        Err(Error::BadManifest(..))
    ));
    assert!(matches!(f.store.runs(), Err(Error::BadManifest(..))));
    assert!(f.root.join("src/main.rs").exists());
}

#[test]
fn run_ids_cannot_escape_the_store() {
    let f = fixture();
    for id in ["../store", "..", "", "runs/../../x", "a/b"] {
        assert!(
            matches!(f.store.load(id), Err(Error::UnknownRun(_))),
            "{id:?}"
        );
        assert!(
            matches!(f.store.purge(id), Err(Error::UnknownRun(_))),
            "{id:?}"
        );
    }
}

#[test]
fn history_lists_runs_newest_first() {
    let f = fixture();
    assert!(f.store.runs().unwrap().is_empty());
    assert!(matches!(f.store.clean(Vec::new()), Err(Error::EmptyPlan)));
    let first = f.store.clean(vec![f.approve("target")]).unwrap();
    let second = f.store.clean(vec![f.approve("node_modules")]).unwrap();
    assert_ne!(first.run_id, second.run_id);
    let runs = f.store.runs().unwrap();
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].run_id, second.run_id);
    assert_eq!(f.store.purge_older_than(1).unwrap().len(), 0);
    assert_eq!(f.store.purge_older_than(0).unwrap().len(), 2);
}
