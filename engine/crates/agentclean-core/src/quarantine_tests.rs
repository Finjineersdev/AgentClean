//! Quarantine round trips against real directories.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use crate::activity::{Activity, Verdict};
use crate::guard::{Approved, Guard};
use crate::quarantine::{Error, Manifest, State, Store, MANIFEST};
use crate::rules::Candidate;

fn idle(root: &Path) -> Activity {
    Activity::unchecked(root, Verdict::Idle, false)
}

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
            .check(&idle(&self.root), &Candidate::unchecked(rel))
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
fn manifests_cannot_send_items_outside_their_project() {
    let f = fixture();
    let run = f.store.clean(vec![f.approve("target")]).unwrap();
    let path = f.store.root().join("runs").join(&run.run_id).join(MANIFEST);
    let outside = f.base.join("elsewhere");
    let cases: [(PathBuf, PathBuf); 4] = [
        (outside.clone(), f.root.clone()),
        (f.root.join("../../elsewhere"), f.root.clone()),
        (f.root.clone(), f.root.clone()),
        (f.root.join("target"), PathBuf::from("work/proj")),
    ];
    for (original, project_root) in cases {
        let mut evil = run.clone();
        evil.entries[0].original = original.clone();
        evil.entries[0].project_root = project_root;
        fs::write(&path, serde_json::to_vec(&evil).unwrap()).unwrap();
        assert!(
            matches!(
                f.store.restore(&run.run_id, None),
                Err(Error::BadManifest(..))
            ),
            "{original:?}"
        );
    }
    assert!(!outside.exists());
}

#[cfg(unix)]
#[test]
fn restore_does_not_follow_a_replaced_parent() {
    let f = fixture();
    fs::create_dir_all(f.root.join("android/app/build")).unwrap();
    let run = f.store.clean(vec![f.approve("android/app/build")]).unwrap();
    assert_eq!(run.entries[0].state, State::Moved);

    // Swap `android/app` for a link to a folder outside the project.
    let outside = f.base.join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::remove_dir_all(f.root.join("android/app")).unwrap();
    std::os::unix::fs::symlink(&outside, f.root.join("android/app")).unwrap();

    let after = f.store.restore(&run.run_id, None).unwrap();
    assert_eq!(after.entries[0].state, State::Moved);
    assert!(after.entries[0].note.is_some());
    assert!(!outside.join("build").exists());
    assert!(f.items(&run).join("0000").is_dir());
}

#[cfg(unix)]
#[test]
fn the_store_may_not_be_a_link() {
    let f = fixture();
    let link = f.base.join("store-link");
    std::os::unix::fs::symlink(f.store.root(), &link).unwrap();
    assert!(matches!(Store::open(&link), Err(Error::Store(..))));
}

#[cfg(unix)]
#[test]
fn purge_removes_only_real_directories() {
    let f = fixture();
    let run = f
        .store
        .clean(vec![f.approve("target"), f.approve("node_modules")])
        .unwrap();
    let items = f.items(&run);
    let keep = f.base.join("keep");
    fs::create_dir_all(keep.join("inner")).unwrap();
    // Item 0 becomes a link to a folder outside the store, item 1 a file.
    fs::remove_dir_all(items.join("0000")).unwrap();
    std::os::unix::fs::symlink(&keep, items.join("0000")).unwrap();
    fs::remove_dir_all(items.join("0001")).unwrap();
    fs::write(items.join("0001"), "x").unwrap();

    let after = f.store.purge(&run.run_id).unwrap();
    for e in &after.entries {
        assert_eq!(e.state, State::Moved);
        assert!(e.note.is_some());
    }
    assert!(keep.join("inner").is_dir());
    assert!(fs::symlink_metadata(items.join("0000")).is_ok());
    assert!(items.join("0001").is_file());
}

#[test]
fn purging_by_age_skips_runs_holding_nothing() {
    let f = fixture();
    let run = f.store.clean(vec![f.approve("target")]).unwrap();
    f.store.restore(&run.run_id, None).unwrap();
    assert!(f.store.purge_older_than(0).unwrap().is_empty());
}

#[test]
fn runs_record_the_current_time() {
    let f = fixture();
    let run = f.store.clean(vec![f.approve("target")]).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert!(now.abs_diff(run.created_unix) < 60, "{}", run.created_unix);
}

#[cfg(unix)]
#[test]
fn an_unwritable_store_fails_with_the_real_reason() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture();
    let runs = f.store.root().join("runs");
    fs::set_permissions(&runs, fs::Permissions::from_mode(0o555)).unwrap();
    let result = f.store.clean(vec![f.approve("target")]);
    fs::set_permissions(&runs, fs::Permissions::from_mode(0o755)).unwrap();
    match result {
        Err(Error::Store(_, why)) => assert!(!why.contains("allocate"), "{why}"),
        other => panic!("{other:?}"),
    }
    assert!(f.root.join("target").is_dir());
}

#[test]
fn a_pending_entry_without_its_item_is_left_alone() {
    let f = fixture();
    let run = f.store.clean(vec![f.approve("target")]).unwrap();
    // Simulate a crash before the move: the item never reached `items/`.
    fs::rename(f.items(&run).join("0000"), f.base.join("elsewhere")).unwrap();
    let mut pending = read_manifest(&f, &run);
    pending.entries[0].state = State::Pending;
    let path = f.store.root().join("runs").join(&run.run_id).join(MANIFEST);
    fs::write(&path, serde_json::to_vec(&pending).unwrap()).unwrap();

    let after = f.store.restore(&run.run_id, None).unwrap();
    assert_eq!(after.entries[0].state, State::Pending);
    assert_eq!(after.entries[0].note, None);
    let after = f.store.purge(&run.run_id).unwrap();
    assert_eq!(after.entries[0].state, State::Pending);
}

#[test]
fn restoring_an_item_past_the_end_is_refused() {
    let f = fixture();
    let run = f.store.clean(vec![f.approve("target")]).unwrap();
    assert!(matches!(
        f.store.restore(&run.run_id, Some(1)),
        Err(Error::UnknownItem(_, 1))
    ));
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

#[cfg(unix)]
#[test]
fn a_drive_quarantine_must_be_a_private_folder_of_the_owner() {
    use crate::quarantine::trusted;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("q");
    fs::create_dir(&dir).unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
    let me = fs::metadata(&dir).unwrap().uid();
    assert_eq!(trusted(&dir, me), Ok(()));
    let other = trusted(&dir, me + 1).unwrap_err();
    assert!(other.contains("another user"), "{other}");
    for mode in [0o720, 0o702, 0o777] {
        fs::set_permissions(&dir, fs::Permissions::from_mode(mode)).unwrap();
        let why = trusted(&dir, me).unwrap_err();
        assert!(why.contains("changed by other users"), "{mode:o}: {why}");
    }
    let link = tmp.path().join("link");
    std::os::unix::fs::symlink(&dir, &link).unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(trusted(&link, me)
        .unwrap_err()
        .contains("not a real folder"));
}

#[cfg(unix)]
#[test]
fn a_shared_drive_top_needs_the_sticky_bit() {
    use crate::quarantine::shared_safely;
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let top = tmp.path();
    for (mode, ok) in [
        (0o755, true),
        (0o1777, true),
        (0o777, false),
        (0o775, false),
    ] {
        fs::set_permissions(top, fs::Permissions::from_mode(mode)).unwrap();
        assert_eq!(shared_safely(top).is_ok(), ok, "{mode:o}");
    }
    fs::set_permissions(top, fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn manifests_cannot_point_at_another_drives_quarantine() {
    let f = fixture();
    let run = f.store.clean(vec![f.approve("target")]).unwrap();
    let path = f.store.root().join("runs").join(&run.run_id).join(MANIFEST);
    let good = fs::read_to_string(&path).unwrap();
    let base = f.base.to_str().unwrap();
    for volume in [
        "/elsewhere/.agentclean-quarantine".to_owned(),
        format!("{base}/work/not-the-store"),
        format!("{base}/work/../.agentclean-quarantine"),
        ".agentclean-quarantine".to_owned(),
    ] {
        let bad = good.replacen(
            "\"state\"",
            &format!("\"volume\": {volume:?},\n      \"state\""),
            1,
        );
        fs::write(&path, bad).unwrap();
        assert!(
            matches!(f.store.load(&run.run_id), Err(Error::BadManifest(..))),
            "{volume}"
        );
    }
    let ok = good.replacen(
        "\"state\"",
        &format!("\"volume\": \"{base}/work/.agentclean-quarantine\",\n      \"state\""),
        1,
    );
    fs::write(&path, ok).unwrap();
    assert!(f.store.load(&run.run_id).is_ok());
}

/// Runs on a real second drive: set AGENTCLEAN_TEST_VOLUME to a writable
/// folder on another local drive (CI uses a disk image on macOS and
/// /dev/shm on Linux).
#[cfg(unix)]
#[test]
fn folders_on_another_drive_use_that_drives_quarantine() {
    use crate::quarantine::VOLUME_STORE;
    use std::os::unix::fs::PermissionsExt;
    let Some(volume) = std::env::var_os("AGENTCLEAN_TEST_VOLUME") else {
        // CI provides a second drive; there, a missing one is a failure.
        assert!(
            std::env::var_os("CI").is_none(),
            "AGENTCLEAN_TEST_VOLUME is not set"
        );
        eprintln!("skipped: AGENTCLEAN_TEST_VOLUME is not set");
        return;
    };
    let f = fixture();
    let work = tempfile::tempdir_in(&volume).unwrap();
    let root = fs::canonicalize(work.path()).unwrap().join("proj");
    fs::create_dir_all(root.join("target/debug")).unwrap();
    fs::write(root.join("target/debug/app"), vec![1u8; 9_000]).unwrap();
    let before = snapshot(&root.join("target"));
    let drive = crate::volumes::drive_root(&root).unwrap();
    let store = drive.join(VOLUME_STORE);
    let approve = |rel: &str| {
        f.guard
            .check(&idle(&root), &Candidate::unchecked(rel))
            .unwrap()
    };

    // Clean, then restore, through the drive's own quarantine.
    let run = f
        .store
        .clean(vec![approve("target"), f.approve("node_modules")])
        .unwrap();
    assert_eq!(
        run.entries[0].state,
        State::Moved,
        "{:?}",
        run.entries[0].note
    );
    assert_eq!(run.entries[0].volume.as_deref(), Some(store.as_path()));
    assert_eq!(run.entries[1].volume, None);
    let held = store.join("runs").join(&run.run_id).join("items/0000");
    assert!(held.is_dir() && !root.join("target").exists());
    assert_eq!(read_manifest(&f, &run), run);
    let back = f.store.restore(&run.run_id, None).unwrap();
    assert_eq!(back.entries[0].state, State::Restored);
    assert_eq!(snapshot(&root.join("target")), before);

    // A drive that is not connected leaves the item where it is, with a note.
    let run = f.store.clean(vec![approve("target")]).unwrap();
    let away = drive.join(".agentclean-away");
    fs::rename(&store, &away).unwrap();
    let kept = f.store.restore(&run.run_id, None).unwrap();
    assert_eq!(kept.entries[0].state, State::Moved);
    assert!(kept.entries[0]
        .note
        .as_deref()
        .unwrap()
        .contains("not connected"));
    fs::rename(&away, &store).unwrap();

    // A quarantine others can change is never used, for moves or for purges.
    fs::set_permissions(&store, fs::Permissions::from_mode(0o777)).unwrap();
    let purged = f.store.purge(&run.run_id).unwrap();
    assert_eq!(purged.entries[0].state, State::Moved);
    fs::create_dir_all(root.join("target")).unwrap();
    let refused = f.store.clean(vec![approve("target")]).unwrap();
    assert_eq!(refused.entries[0].state, State::Skipped);
    assert!(root.join("target").is_dir());
    fs::set_permissions(&store, fs::Permissions::from_mode(0o700)).unwrap();

    let purged = f.store.purge(&run.run_id).unwrap();
    assert_eq!(purged.entries[0].state, State::Purged);
    assert!(!store
        .join("runs")
        .join(&run.run_id)
        .join("items/0000")
        .exists());
}
