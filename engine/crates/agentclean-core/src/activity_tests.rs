#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use crate::activity::{assess, Activity, Config, Processes, Verdict};
use crate::rules::{load_dir, RuleFile};

const DAY: Duration = Duration::from_secs(86_400);

fn rules() -> Vec<RuleFile> {
    load_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules")).unwrap()
}

fn project() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap().join("proj");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("package.json"), "{}").unwrap();
    fs::write(root.join("src/index.js"), "1").unwrap();
    (tmp, root)
}

fn touch(path: &Path, days_ago: u64) {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).unwrap();
    }
    if !path.exists() {
        fs::write(path, "x").unwrap();
    }
    let t = SystemTime::now() - DAY * days_ago as u32;
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(t)
        .unwrap();
}

fn age_all(root: &Path, days_ago: u64) {
    for p in ["package.json", "src/index.js"] {
        touch(&root.join(p), days_ago);
    }
}

fn run(root: &Path, procs: Option<&Processes>) -> Activity {
    assess(root, &rules(), &Config::default(), procs, SystemTime::now())
}

fn quiet() -> Processes {
    Processes::default()
}

#[test]
fn edits_decide_the_verdict() {
    let (_tmp, root) = project();
    let q = quiet();
    let a = run(&root, Some(&q));
    assert_eq!((a.verdict(), a.reason()), (Verdict::Active, "edited today"));
    assert_eq!(a.root(), root);
    assert!(!a.pinned());

    age_all(&root, 13);
    assert_eq!(run(&root, Some(&q)).reason(), "edited 13 days ago");
    assert_eq!(run(&root, Some(&q)).verdict(), Verdict::Active);
    age_all(&root, 14);
    assert_eq!(run(&root, Some(&q)).verdict(), Verdict::Idle);
    age_all(&root, 89);
    assert_eq!(run(&root, Some(&q)).verdict(), Verdict::Idle);
    age_all(&root, 90);
    let a = run(&root, Some(&q));
    assert_eq!(
        (a.verdict(), a.reason()),
        (Verdict::Dormant, "edited 90 days ago")
    );
    age_all(&root, 1);
    assert_eq!(run(&root, Some(&q)).reason(), "edited 1 day ago");
}

#[test]
fn build_output_hidden_folders_and_links_are_not_edits() {
    let (_tmp, root) = project();
    age_all(&root, 200);
    touch(&root.join("node_modules/x/index.js"), 0);
    touch(&root.join(".idea/workspace.xml"), 0);
    touch(&root.join("App.xcodeproj/project.pbxproj"), 0);
    // Build output of a project nested inside is skipped too.
    touch(&root.join("tools/package.json"), 200);
    touch(&root.join("tools/node_modules/y/index.js"), 0);
    #[cfg(unix)]
    std::os::unix::fs::symlink(std::env::temp_dir(), root.join("tmp-link")).unwrap();
    assert_eq!(run(&root, Some(&quiet())).verdict(), Verdict::Dormant);
    // A hidden file at the top is the user's own: it counts.
    touch(&root.join(".env.example"), 0);
    assert_eq!(run(&root, Some(&quiet())).verdict(), Verdict::Active);
}

#[test]
fn an_incomplete_check_is_never_dormant() {
    let (_tmp, root) = project();
    age_all(&root, 200);
    let a = run(&root, None);
    assert_eq!(a.verdict(), Verdict::Idle);
    assert!(
        a.reason().contains("running tools could not be checked"),
        "{}",
        a.reason()
    );

    // Fifteen folders down is walked in full; the sixteenth is beyond the limit.
    let deep = "a/b/c/d/e/f/g/h/i/j/k/l/m/n/o";
    touch(&root.join(deep).join("ok.txt"), 200);
    assert_eq!(run(&root, Some(&quiet())).verdict(), Verdict::Dormant);
    touch(&root.join(deep).join("p/deep.txt"), 200);
    let a = run(&root, Some(&quiet()));
    assert_eq!(a.verdict(), Verdict::Idle);
    assert!(a.reason().contains("too many files"), "{}", a.reason());
}

#[test]
fn too_many_files_is_never_dormant() {
    let (_tmp, root) = project();
    age_all(&root, 200);
    let many = root.join("many");
    fs::create_dir_all(&many).unwrap();
    // package.json, src, src/index.js and many: 4 entries, plus the files.
    for i in 0..19_996 {
        touch(&many.join(i.to_string()), 200);
    }
    assert_eq!(run(&root, Some(&quiet())).verdict(), Verdict::Dormant);
    touch(&many.join("one-more"), 200);
    let a = run(&root, Some(&quiet()));
    assert_eq!(a.verdict(), Verdict::Idle, "{}", a.reason());
}

#[test]
fn nothing_found_is_idle() {
    let tmp = tempfile::tempdir().unwrap();
    let a = run(tmp.path(), Some(&quiet()));
    assert_eq!(
        (a.verdict(), a.reason()),
        (Verdict::Idle, "no edits or commits found")
    );
}

fn git(root: &Path, args: &[&str], days_ago: u64) {
    let date = format!(
        "@{} +0000",
        (SystemTime::now() - DAY * days_ago as u32)
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    );
    let ok = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(root)
        .env("GIT_COMMITTER_DATE", &date)
        .env("GIT_AUTHOR_DATE", &date)
        .output()
        .unwrap();
    assert!(
        ok.status.success(),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
}

#[test]
fn the_last_commit_touching_the_project_counts() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = fs::canonicalize(tmp.path()).unwrap();
    for p in ["web/package.json", "api/package.json"] {
        touch(&repo.join(p), 300);
    }
    git(&repo, &["init", "-q"], 0);
    git(&repo, &["add", "."], 0);
    git(&repo, &["commit", "-qm", "one"], 200);
    let q = quiet();
    let web = repo.join("web");
    let a = run(&web, Some(&q));
    assert_eq!(
        (a.verdict(), a.reason()),
        (Verdict::Dormant, "last commit 200 days ago")
    );

    // A newer commit elsewhere in the repository does not count.
    touch(&repo.join("api/package.json"), 300);
    fs::write(repo.join("api/package.json"), "{\"a\":1}").unwrap();
    touch(&repo.join("api/package.json"), 300);
    git(&repo, &["commit", "-qam", "two"], 3);
    assert_eq!(run(&web, Some(&q)).verdict(), Verdict::Dormant);
    let a = run(&repo.join("api"), Some(&q));
    assert_eq!(
        (a.verdict(), a.reason()),
        (Verdict::Active, "last commit 3 days ago")
    );
}

#[test]
fn a_broken_repository_is_never_dormant() {
    let (_tmp, root) = project();
    age_all(&root, 200);
    fs::write(root.join(".git"), "gitdir: /nonexistent").unwrap();
    touch(&root.join(".git"), 200);
    let a = run(&root, Some(&quiet()));
    assert_eq!(a.verdict(), Verdict::Idle);
    assert!(
        a.reason().contains("git could not be read"),
        "{}",
        a.reason()
    );
}

#[cfg(unix)]
#[test]
fn reading_commits_does_not_run_repository_programs() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let repo = fs::canonicalize(tmp.path()).unwrap().join("repo");
    touch(&repo.join("package.json"), 300);
    git(&repo, &["init", "-q"], 0);
    git(&repo, &["add", "."], 0);
    git(&repo, &["commit", "-qm", "one"], 200);
    // Re-create HEAD as a commit that carries a signature, so that
    // log.showSignature would hand it to the repository's gpg.program.
    let head = Command::new("git")
        .args(["cat-file", "commit", "HEAD"])
        .current_dir(&repo)
        .output()
        .unwrap();
    let text = String::from_utf8(head.stdout).unwrap();
    let (headers, body) = text.split_once("\n\n").unwrap();
    let signed = format!(
        "{headers}\ngpgsig -----BEGIN PGP SIGNATURE-----\n \n iQ==\n -----END PGP SIGNATURE-----\n\n{body}"
    );
    fs::write(tmp.path().join("commit"), signed).unwrap();
    let id = Command::new("git")
        .args(["hash-object", "-t", "commit", "-w", "../commit"])
        .current_dir(&repo)
        .output()
        .unwrap();
    let id = String::from_utf8(id.stdout).unwrap();
    git(&repo, &["update-ref", "HEAD", id.trim()], 0);

    let marker = tmp.path().join("ran");
    let script = tmp.path().join("gpg.sh");
    fs::write(
        &script,
        format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    git(&repo, &["config", "log.showSignature", "true"], 0);
    git(
        &repo,
        &["config", "gpg.program", script.to_str().unwrap()],
        0,
    );

    // The trap is armed: plain git log runs the program.
    Command::new("git")
        .args(["log", "-1"])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert!(marker.exists(), "test setup: gpg.program did not run");
    fs::remove_file(&marker).unwrap();

    let a = run(&repo, Some(&quiet()));
    assert_eq!(a.reason(), "last commit 200 days ago");
    assert!(!marker.exists(), "assess ran the repository's gpg.program");
}

#[cfg(unix)]
#[test]
fn reading_commits_does_not_fetch_from_the_repositorys_remote() {
    let tmp = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(tmp.path()).unwrap();
    let repo = base.join("repo");
    touch(&repo.join("package.json"), 200);
    git(&repo, &["init", "-q"], 0);
    // HEAD's tree is missing, as in a partial clone, and the promisor remote
    // is an ext:: transport that runs a command.
    let commit = "tree 1111111111111111111111111111111111111111\n\
        author t <t@example.invalid> 1600000000 +0000\n\
        committer t <t@example.invalid> 1600000000 +0000\n\nx\n";
    fs::write(base.join("commit"), commit).unwrap();
    let id = Command::new("git")
        .args([
            "hash-object",
            "-t",
            "commit",
            "-w",
            "--literally",
            "../commit",
        ])
        .current_dir(&repo)
        .output()
        .unwrap();
    let id = String::from_utf8(id.stdout).unwrap();
    git(&repo, &["update-ref", "HEAD", id.trim()], 0);
    let marker = base.join("ran");
    let url = format!("ext::sh -c touch% {}", marker.display());
    for (key, value) in [
        ("core.repositoryformatversion", "1"),
        ("extensions.partialClone", "origin"),
        ("remote.origin.url", url.as_str()),
        ("remote.origin.promisor", "true"),
        ("protocol.ext.allow", "always"),
        ("protocol.allow", "always"),
    ] {
        git(&repo, &["config", key, value], 0);
    }

    // The trap is armed: plain git log runs the remote's command.
    Command::new("git")
        .args(["log", "-1", "--", "."])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert!(marker.exists(), "test setup: the lazy fetch did not run");
    fs::remove_file(&marker).unwrap();

    let a = run(&repo, Some(&quiet()));
    assert!(
        !marker.exists(),
        "assess fetched from the repository's remote"
    );
    assert!(
        a.reason().contains("git could not be read"),
        "{}",
        a.reason()
    );
    assert_ne!(a.verdict(), Verdict::Dormant);
}

#[cfg(unix)]
#[test]
fn a_tool_running_inside_makes_it_active() {
    let (_tmp, root) = project();
    age_all(&root, 200);
    // Started through a shell that exits at once, in a process group of its
    // own, as a shell's background job is: our own children and process
    // group are ignored (lsof and our pipeline run in our folder).
    use std::os::unix::process::CommandExt;
    let out = Command::new("sh")
        .process_group(0)
        .args(["-c", "sleep 30 >/dev/null 2>&1 & echo $!"])
        .current_dir(root.join("src"))
        .output()
        .unwrap();
    let pid = String::from_utf8(out.stdout).unwrap().trim().to_owned();
    let procs = Processes::snapshot();
    Command::new("kill").arg(&pid).status().unwrap();
    let a = run(&root, procs.as_ref());
    assert_eq!(a.verdict(), Verdict::Active, "{}", a.reason());
    assert_eq!(
        a.reason(),
        format!("sleep (pid {pid}) is running in this folder")
    );
    // This test process runs elsewhere and is not counted anyway.
    assert_eq!(
        run(&root, Processes::snapshot().as_ref()).verdict(),
        Verdict::Dormant
    );
}

#[test]
fn pins_cover_the_folder_and_everything_inside() {
    let (_tmp, root) = project();
    age_all(&root, 200);
    let cfg = Config {
        pinned: vec![root.parent().unwrap().to_path_buf()],
        ..Config::default()
    };
    let a = assess(&root, &rules(), &cfg, Some(&quiet()), SystemTime::now());
    assert!(a.pinned());
    assert_eq!(a.reason(), "pinned; edited 200 days ago");
    let other = Config {
        pinned: vec![root.join("src")],
        ..Config::default()
    };
    assert!(!assess(&root, &rules(), &other, Some(&quiet()), SystemTime::now()).pinned());
}

#[test]
fn config_is_read_strictly() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("sub/config.toml");
    assert_eq!(Config::load(&path).unwrap(), Config::default());

    let cfg = Config {
        idle_days: 7,
        dormant_days: 30,
        pinned: vec![PathBuf::from("/w/keep")],
        exclude: vec![PathBuf::from("/w/sdk")],
        pinned_until: [(PathBuf::from("/w/keep"), 1_900_000_000)]
            .into_iter()
            .collect(),
        pinned_remotes: vec!["github.com/me/app".to_owned()],
    };
    cfg.save(&path).unwrap();
    assert_eq!(Config::load(&path).unwrap(), cfg);
    assert!(!tmp.path().join("sub/config.toml.tmp").exists());

    for bad in [
        "pinned = [\"relative/path\"]",
        "exclude = [\"relative/path\"]",
        "pined = []",
        "idle_days = 30\ndormant_days = 7",
        "idle_days = -1",
    ] {
        fs::write(&path, bad).unwrap();
        assert!(Config::load(&path).is_err(), "{bad}");
    }
    fs::write(&path, "idle_days = 30\ndormant_days = 30").unwrap();
    assert!(Config::load(&path).is_ok());
    // A config that exists but cannot be read is an error, not the default.
    assert!(Config::load(tmp.path()).is_err());
}

/// P9: a pin can end on a date; after it, the project is judged as usual.
#[test]
fn a_pin_until_a_date_ends() {
    let (_t, root) = project();
    age_all(&root, 200);
    let now = SystemTime::now();
    let secs = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let with_until = |until: u64| Config {
        pinned: vec![root.clone()],
        pinned_until: [(root.clone(), until)].into_iter().collect(),
        ..Config::default()
    };
    assert!(assess(
        &root,
        &rules(),
        &with_until(secs + 86_400),
        Some(&quiet()),
        now
    )
    .pinned());
    assert!(!assess(&root, &rules(), &with_until(secs - 1), Some(&quiet()), now).pinned());
}

/// P9: a pin on a git remote follows the project to any folder, whatever
/// form the URL takes.
#[test]
fn a_pin_on_a_git_remote_follows_the_project() {
    let (_t, root) = project();
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::write(
        root.join(".git/config"),
        "[core]\n\tbare = false\n[remote \"upstream\"]\n\turl = https://example.com/other/thing\n[remote \"origin\"]\n\turl = git@GitHub.com:me/app.git\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n",
    )
    .unwrap();
    assert_eq!(
        crate::activity::git_remote(&root).as_deref(),
        Some("github.com/me/app")
    );
    let cfg = |remote: &str| Config {
        pinned_remotes: vec![remote.to_owned()],
        ..Config::default()
    };
    for same in [
        "https://github.com/me/app",
        "ssh://git@github.com/me/app.git/",
        "github.com/me/app",
    ] {
        assert!(
            assess(
                &root,
                &rules(),
                &cfg(same),
                Some(&quiet()),
                SystemTime::now()
            )
            .pinned(),
            "{same}"
        );
    }
    assert!(!assess(
        &root,
        &rules(),
        &cfg("github.com/me/other"),
        Some(&quiet()),
        SystemTime::now()
    )
    .pinned());
    // No repository, no remote pin.
    fs::remove_dir_all(root.join(".git")).unwrap();
    assert!(!assess(
        &root,
        &rules(),
        &cfg("github.com/me/app"),
        Some(&quiet()),
        SystemTime::now()
    )
    .pinned());
}

#[test]
fn old_config_files_still_load() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("config.toml");
    fs::write(
        &path,
        "idle_days = 7\ndormant_days = 60\npinned = [\"/w/keep\"]\nexclude = []\n",
    )
    .unwrap();
    let cfg = Config::load(&path).unwrap();
    assert!(cfg.pinned_until.is_empty() && cfg.pinned_remotes.is_empty());
}

/// P8: a repository's `.agentclean.toml` can pin it or keep folders, nothing
/// else; anything it cannot say plainly pins the project (hostile input).
#[test]
fn a_repository_file_can_only_take_folders_away() {
    let (_t, root) = project();
    age_all(&root, 200);
    let run = || {
        assess(
            &root,
            &rules(),
            &Config::default(),
            Some(&quiet()),
            SystemTime::now(),
        )
    };
    assert!(!run().pinned() && run().kept().is_empty());

    fs::write(
        root.join(".agentclean.toml"),
        "exclude = [\"dist\", \"data/cache/\"]\n",
    )
    .unwrap();
    assert!(!run().pinned());
    assert_eq!(
        run().kept(),
        [PathBuf::from("dist"), PathBuf::from("data/cache")]
    );

    fs::write(root.join(".agentclean.toml"), "pin = true\n").unwrap();
    assert!(run().pinned());
    assert!(
        run().reason().contains(".agentclean.toml"),
        "{}",
        run().reason()
    );

    for bad in [
        "[[artifact]]\npath = \"src\"\n",
        "exclude = [\"/etc\"]\n",
        "exclude = [\"../other\"]\n",
        "pin = \"yes\"\n",
        "not toml",
    ] {
        fs::write(root.join(".agentclean.toml"), bad).unwrap();
        assert!(run().pinned(), "{bad}");
    }
}
