//! Guard invariants, exercised against real directories.
//!
//! These live inside the crate because they need `Candidate::unchecked`, a
//! test-only constructor that bypasses the rule linter so the guard's own
//! checks can be attacked directly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use proptest::prelude::*;
use tempfile::TempDir;

use crate::activity::{Activity, Verdict};
use crate::deny::{protected_component, protected_target};
use crate::guard::{Approved, Guard, Refusal};
use crate::rules::Candidate;

fn idle(root: &Path) -> Activity {
    Activity::unchecked(root, Verdict::Idle, false)
}

/// A project at `<tmp>/work/proj` with a fake home at `<tmp>/home`.
struct Fixture {
    _tmp: TempDir,
    base: PathBuf,
    root: PathBuf,
    guard: Guard,
}

impl Fixture {
    fn check(&self, rel: &str) -> Result<Approved, Refusal> {
        self.guard
            .check(&idle(&self.root), &Candidate::unchecked(rel))
    }
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(tmp.path()).unwrap();
    let root = base.join("work/proj");
    let home = base.join("home");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&home).unwrap();
    for d in [
        "node_modules/pkg",
        "target/debug",
        "src",
        "config/.env.d",
        "android/app/build",
    ] {
        fs::create_dir_all(root.join(d)).unwrap();
    }
    fs::write(root.join(".env"), "SECRET=1").unwrap();
    fs::write(root.join("package.json"), "{}").unwrap();
    Fixture {
        guard: Guard::new(Some(home)),
        _tmp: tmp,
        base,
        root,
    }
}

fn git(root: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
        .args(args)
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok, "git {args:?} failed");
}

#[test]
fn approves_plain_artifact_directories() {
    let f = fixture();
    for d in ["node_modules", "target", "android/app/build"] {
        let ok = f.check(d).unwrap();
        assert_eq!(ok.path(), f.root.join(d));
        assert_eq!(ok.project_root(), f.root);
        assert_eq!(ok.rule_id(), "test");
        ok.reverify().unwrap();
    }
}

#[test]
fn refuses_paths_that_are_not_plain_relative() {
    let f = fixture();
    for rel in [
        "",
        ".",
        "..",
        "../proj/target",
        "node_modules/../src",
        "/etc",
        "./target",
    ] {
        assert_eq!(f.check(rel), Err(Refusal::NotNormalized), "{rel:?}");
    }
    assert_eq!(f.check("missing"), Err(Refusal::NotFound));
    let relative_root = f.guard.check(
        &idle(Path::new("work/proj")),
        &Candidate::unchecked("target"),
    );
    assert_eq!(relative_root, Err(Refusal::NotAbsolute));
    // The root itself must be normalized too, even though it resolves.
    let dotted_root = f.guard.check(
        &idle(&f.root.join("../proj")),
        &Candidate::unchecked("target"),
    );
    assert_eq!(dotted_root, Err(Refusal::NotNormalized));
}

#[test]
fn refuses_protected_names_and_files() {
    let f = fixture();
    assert!(matches!(f.check("src"), Err(Refusal::Protected(..))));
    assert!(matches!(
        f.check("android/app"),
        Err(Refusal::Protected(..))
    ));
    assert!(matches!(
        f.check("config/.env.d"),
        Err(Refusal::Protected(..))
    ));
    assert_eq!(f.check(".env"), Err(Refusal::NotADirectory));
    assert_eq!(f.check("package.json"), Err(Refusal::NotADirectory));
    fs::create_dir_all(f.root.join("node_modules/.git")).unwrap();
    assert_eq!(f.check("node_modules"), Err(Refusal::ContainsRepository));
}

#[test]
fn refuses_home_and_shallow_roots() {
    let f = fixture();
    let home = f.base.join("home");
    fs::create_dir_all(home.join("node_modules")).unwrap();
    let c = Candidate::unchecked("node_modules");
    assert_eq!(
        f.guard.check(&idle(&home), &c),
        Err(Refusal::ProjectRootTooBroad)
    );
    // `base` contains the fake home, so it is too broad whatever its depth.
    assert_eq!(
        f.guard.check(&idle(&f.base), &Candidate::unchecked("work")),
        Err(Refusal::ProjectRootTooBroad)
    );
    assert_eq!(
        f.guard
            .check(&idle(Path::new("/")), &Candidate::unchecked("usr")),
        Err(Refusal::ProjectRootTooBroad)
    );
    // Without a home folder to compare against, depth alone refuses `/`.
    assert_eq!(
        Guard::new(None).check(&idle(Path::new("/")), &Candidate::unchecked("usr")),
        Err(Refusal::ProjectRootTooBroad)
    );
}

#[cfg(unix)]
#[test]
fn refuses_symlinks_in_every_position() {
    use std::os::unix::fs::symlink;
    let f = fixture();
    let outside = f.base.join("outside/precious");
    fs::create_dir_all(&outside).unwrap();

    // The target itself is a link out of the project.
    symlink(&outside, f.root.join("dist")).unwrap();
    assert_eq!(f.check("dist"), Err(Refusal::Symlink));

    // A parent component is a link out of the project.
    symlink(f.base.join("outside"), f.root.join("vendor")).unwrap();
    assert_eq!(f.check("vendor/precious"), Err(Refusal::OutsideProject));

    // A parent component is a link to elsewhere inside the project.
    symlink(f.root.join("node_modules"), f.root.join("alias")).unwrap();
    assert_eq!(f.check("alias/pkg"), Err(Refusal::SymlinkInPath));

    // Reaching the project through a linked root is fine: the root is resolved.
    symlink(&f.root, f.base.join("shortcut")).unwrap();
    let ok = f
        .guard
        .check(
            &idle(&f.base.join("shortcut")),
            &Candidate::unchecked("target"),
        )
        .unwrap();
    assert_eq!(ok.path(), f.root.join("target"));
}

#[cfg(unix)]
#[test]
fn reverify_catches_a_swap_after_approval() {
    use std::os::unix::fs::symlink;
    let f = fixture();
    let target = f.root.join("target");

    // Replaced by a fresh directory of the same name (the inode may be reused).
    let approved = f.check("target").unwrap();
    fs::remove_dir_all(&target).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    fs::create_dir(&target).unwrap();
    assert_eq!(approved.reverify(), Err(Refusal::ChangedSinceApproval));

    // Replaced by a link to source.
    let approved = f.check("target").unwrap();
    fs::remove_dir_all(&target).unwrap();
    symlink(f.root.join("src"), &target).unwrap();
    assert_eq!(approved.reverify(), Err(Refusal::ChangedSinceApproval));

    // Removed.
    let approved = f.check("node_modules").unwrap();
    fs::remove_dir_all(f.root.join("node_modules")).unwrap();
    assert_eq!(approved.reverify(), Err(Refusal::ChangedSinceApproval));

    // The same directory, reached through a parent that is now a link: its
    // identity matches but its real path does not.
    let approved = f.check("android/app/build").unwrap();
    fs::rename(f.root.join("android"), f.base.join("moved")).unwrap();
    symlink(f.base.join("moved"), f.root.join("android")).unwrap();
    assert_eq!(approved.reverify(), Err(Refusal::ChangedSinceApproval));
}

#[test]
fn refuses_directories_with_tracked_files() {
    let f = fixture();
    fs::create_dir_all(f.root.join("dist")).unwrap();
    fs::write(f.root.join("dist/bundle.js"), "x").unwrap();
    fs::write(f.root.join("target/debug/out"), "x").unwrap();
    git(&f.root, &["init", "-q"]);
    git(&f.root, &["add", "-f", "dist/bundle.js", "package.json"]);
    git(&f.root, &["commit", "-q", "-m", "init"]);

    assert_eq!(f.check("dist"), Err(Refusal::ContainsTrackedFiles));
    assert!(f.check("target").is_ok());
    assert!(f.check("node_modules").is_ok());
}

#[test]
fn refuses_when_repository_cannot_be_read() {
    let f = fixture();
    // Something named `.git` that git cannot use: refuse, do not guess.
    fs::write(f.root.join(".git"), "not a repository").unwrap();
    assert_eq!(f.check("target"), Err(Refusal::VcsCheckFailed));
}

#[cfg(unix)]
#[test]
fn git_check_does_not_run_repository_programs() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture();
    git(&f.root, &["init", "-q"]);
    let marker = f.base.join("pwned");
    let script = f.base.join("evil.sh");
    fs::write(&script, format!("#!/bin/sh\ntouch {}\n", marker.display())).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    git(
        &f.root,
        &["config", "core.fsmonitor", script.to_str().unwrap()],
    );

    assert!(f.check("target").is_ok());
    assert!(
        !marker.exists(),
        "the guard let git run a program named by the repository"
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    /// Whatever path is asked for, an approval always names a real directory
    /// strictly inside the project, with no protected component.
    #[test]
    fn approval_implies_invariants(parts in prop::collection::vec(
        prop::sample::select(vec![
            "node_modules", "pkg", "target", "debug", "src", ".git", "..", ".", "config",
            ".env.d", ".env", "android", "app", "build", "missing", "package.json", "/",
        ]),
        0..5,
    )) {
        let f = fixture();
        let rel = parts.join("/");
        if let Ok(ok) = f.check(&rel) {
            let canon = fs::canonicalize(f.root.join(&rel)).unwrap();
            prop_assert_eq!(ok.path(), canon.as_path());
            prop_assert!(canon.starts_with(&f.root) && canon != f.root);
            prop_assert!(fs::symlink_metadata(&canon).unwrap().is_dir());
            let inside = canon.strip_prefix(&f.root).unwrap();
            for c in inside.components() {
                let name = c.as_os_str().to_str().unwrap();
                prop_assert!(protected_component(name).is_none(), "{}", name);
            }
            let last = inside.file_name().unwrap().to_str().unwrap();
            prop_assert!(protected_target(last).is_none(), "{}", last);
        }
    }
}

#[test]
fn active_and_pinned_projects_are_refused() {
    let f = fixture();
    let c = Candidate::unchecked("target");
    let active = Activity::unchecked(&f.root, Verdict::Active, false);
    assert_eq!(
        f.guard.check(&active, &c),
        Err(Refusal::ActiveProject("test".to_owned()))
    );
    let dormant = Activity::unchecked(&f.root, Verdict::Dormant, false);
    assert!(f.guard.check(&dormant, &c).is_ok());

    let anyway = f.guard.clone().allow_active();
    assert!(anyway.check(&active, &c).is_ok());
    for verdict in [Verdict::Active, Verdict::Idle, Verdict::Dormant] {
        let pinned = Activity::unchecked(&f.root, verdict, true);
        assert_eq!(anyway.check(&pinned, &c), Err(Refusal::Pinned));
        assert_eq!(f.guard.check(&pinned, &c), Err(Refusal::Pinned));
    }
}
