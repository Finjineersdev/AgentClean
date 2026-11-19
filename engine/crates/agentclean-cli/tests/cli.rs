//! End-to-end tests of the `agentclean` binary.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const RULES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../rules");

struct Env {
    _tmp: tempfile::TempDir,
    home: PathBuf,
    project: PathBuf,
    store: PathBuf,
}

fn env() -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(tmp.path()).unwrap();
    let home = base.join("home");
    let project = base.join("work/site");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(project.join("node_modules/react")).unwrap();
    fs::create_dir_all(project.join(".next/cache")).unwrap();
    fs::create_dir_all(project.join("src")).unwrap();
    fs::write(project.join("package.json"), "{}").unwrap();
    fs::write(
        project.join("node_modules/react/index.js"),
        "x".repeat(5000),
    )
    .unwrap();
    fs::write(project.join(".next/cache/build.bin"), vec![0u8; 30_000]).unwrap();
    fs::write(project.join("src/page.tsx"), "export default 1").unwrap();
    Env {
        store: base.join("store"),
        _tmp: tmp,
        home,
        project,
    }
}

impl Env {
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_agentclean"))
            .args(args)
            .env("HOME", &self.home)
            .env_remove("XDG_DATA_HOME")
            .output()
            .unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        let text = String::from_utf8(out.stdout).unwrap();
        assert!(
            out.status.success(),
            "{args:?}\n{text}\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        text
    }

    fn clean(&self, extra: &[&str]) -> String {
        let store = self.store.to_str().unwrap();
        let project = self.project.to_str().unwrap();
        let mut args = vec!["clean", "--rules", RULES, "--store", store];
        args.extend_from_slice(extra);
        args.push(project);
        self.ok(&args)
    }

    fn store_cmd(&self, cmd: &[&str]) -> String {
        let mut args = cmd.to_vec();
        args.extend_from_slice(&["--store", self.store.to_str().unwrap()]);
        self.ok(&args)
    }
}

fn run_id(output: &str) -> String {
    output
        .lines()
        .find_map(|l| l.strip_prefix("run: "))
        .unwrap_or_else(|| panic!("no run id in:\n{output}"))
        .to_owned()
}

fn exists(p: &Path, rel: &str) -> bool {
    p.join(rel).exists()
}

#[test]
fn clean_is_a_dry_run_by_default() {
    let e = env();
    let out = e.clean(&["--include-caution"]);
    assert!(out.contains("Dry run"), "{out}");
    assert!(exists(&e.project, "node_modules/react/index.js"));
    assert!(exists(&e.project, ".next/cache/build.bin"));
    assert!(
        !e.store.exists(),
        "a dry run must not even create the quarantine"
    );
}

#[test]
fn a_folder_two_rules_claim_is_offered_once() {
    let e = env();
    let p = &e.project;
    fs::write(p.join("Cargo.toml"), "[package]").unwrap();
    fs::write(p.join("pom.xml"), "<project/>").unwrap();
    fs::create_dir_all(p.join("target/debug")).unwrap();
    let out = e.clean(&[]);
    assert_eq!(out.matches("target").count(), 1, "{out}");
    assert!(out.contains("2 folders"), "{out}");
}

#[test]
fn caution_folders_need_an_explicit_flag() {
    let e = env();
    let out = e.clean(&["--execute"]);
    assert!(out.contains("--include-caution"), "{out}");
    assert!(!exists(&e.project, ".next"));
    assert!(exists(&e.project, "node_modules/react/index.js"));
}

#[test]
fn clean_history_restore_purge() {
    let e = env();
    let out = e.clean(&["--execute", "--include-caution"]);
    let id = run_id(&out);
    assert!(!exists(&e.project, ".next") && !exists(&e.project, "node_modules"));
    assert!(exists(&e.project, "src/page.tsx"));

    let hist = e.store_cmd(&["history"]);
    assert!(hist.contains(&id), "{hist}");

    e.store_cmd(&["restore", &id]);
    assert!(exists(&e.project, "node_modules/react/index.js"));
    assert!(exists(&e.project, ".next/cache/build.bin"));

    // Clean again, then purge: dry run first, then for real.
    let id = run_id(&e.clean(&["--execute", "--include-caution"]));
    let dry = e.store_cmd(&["purge", &id]);
    assert!(dry.contains("Dry run"), "{dry}");
    let items = e.store.join("runs").join(&id).join("items");
    assert!(exists(&items, "0000"));

    let done = e.store_cmd(&["purge", &id, "--execute"]);
    assert!(done.contains("Freed"), "{done}");
    assert!(!exists(&items, "0000") && !exists(&items, "0001"));
    assert!(exists(&e.project, "src/page.tsx"));
    assert!(
        !exists(&e.project, "node_modules"),
        "purge must not restore"
    );
}

#[test]
fn bad_run_ids_fail_cleanly() {
    let e = env();
    let out = e.run(&["restore", "../../etc", "--store", e.store.to_str().unwrap()]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no run named"));
}

#[test]
fn source_folders_are_never_offered() {
    let e = env();
    let out = e.clean(&["--execute", "--include-caution"]);
    assert!(!out.contains("src"), "{out}");
    assert!(exists(&e.project, "src/page.tsx"));
}

#[test]
fn the_default_store_lives_in_the_home_folder() {
    let e = env();
    e.ok(&["history"]);
    let expected = if cfg!(target_os = "macos") {
        "Library/Application Support/AgentClean/Quarantine/runs"
    } else {
        ".local/share/agentclean/quarantine/runs"
    };
    assert!(e.home.join(expected).is_dir(), "{expected}");
}
