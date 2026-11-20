#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::caches::{self, Env};

fn env(home: &Path, macos: bool) -> Env {
    Env {
        home: home.to_path_buf(),
        macos,
        xdg_cache: None,
        xdg_data: None,
        npm_cache: None,
        gomodcache: None,
        gopath: None,
        cargo_home: None,
        path: vec![home.join("bin")],
    }
}

fn tool(home: &Path, name: &str, script: &str) {
    use std::os::unix::fs::PermissionsExt;
    let bin = home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let p = bin.join(name);
    fs::write(&p, format!("#!/bin/sh\n{script}\n")).unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
}

/// I20 (F15): no default cache on either platform is inside or above a
/// toolchain folder, and a cache pointed into one by the environment is dropped.
#[test]
fn toolchains_never_appear_as_caches() {
    for macos in [true, false] {
        let e = env(Path::new("/Users/dev"), macos);
        let bad = e.toolchains();
        let all = caches::known(&e);
        assert!(all.len() >= 10, "{macos}");
        for c in &all {
            assert!(
                !bad.iter()
                    .any(|t| c.path.starts_with(t) || t.starts_with(&c.path)),
                "{} at {}",
                c.id,
                c.path.display()
            );
        }
        for c in &all {
            assert!(
                !c.path.starts_with("/opt/homebrew/Cellar")
                    && !c.path.starts_with("/Users/dev/.nvm")
            );
        }
    }
    let mut e = env(Path::new("/Users/dev"), true);
    e.gomodcache = Some(PathBuf::from("/Users/dev/.nvm/versions"));
    e.npm_cache = Some(PathBuf::from("/opt/homebrew/Cellar/node"));
    let ids: Vec<_> = caches::known(&e).into_iter().map(|c| c.id).collect();
    assert!(!ids.contains(&"go-mod"), "{ids:?}");
    assert!(!ids.contains(&"npm"), "{ids:?}");
}

#[test]
fn the_home_folder_and_its_parents_are_never_a_cache() {
    let home = Path::new("/Users/dev");
    assert!(!caches::allowed(home, home, &[]));
    assert!(!caches::allowed(Path::new("/Users"), home, &[]));
    assert!(!caches::allowed(Path::new("/"), home, &[]));
    assert!(!caches::allowed(Path::new("relative/cache"), home, &[]));
    assert!(caches::allowed(
        Path::new("/Users/dev/.npm/_cacache"),
        home,
        &[]
    ));
}

#[test]
fn only_caches_on_disk_are_listed_and_links_are_not_followed() {
    let tmp = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(tmp.path()).unwrap();
    fs::create_dir_all(home.join(".npm/_cacache")).unwrap();
    fs::write(home.join(".npm/_cacache/blob"), vec![1u8; 20_000]).unwrap();
    fs::create_dir_all(home.join("elsewhere")).unwrap();
    fs::create_dir_all(home.join(".gradle")).unwrap();
    std::os::unix::fs::symlink(home.join("elsewhere"), home.join(".gradle/caches")).unwrap();
    let found = caches::list(&env(&home, true));
    let ids: Vec<_> = found.iter().map(|f| f.cache.id).collect();
    assert_eq!(ids, ["npm"]);
    assert!(found[0].bytes >= 20_000);
    assert!(found[0].program.is_none(), "npm is not installed here");
}

/// The tool's own command runs with its fixed arguments, from the home
/// folder, and nothing else touches the cache.
#[test]
fn cleaning_runs_the_tools_own_command() {
    let tmp = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(tmp.path()).unwrap();
    fs::create_dir_all(home.join(".npm/_cacache")).unwrap();
    fs::write(home.join(".npm/_cacache/blob"), b"x").unwrap();
    tool(
        &home,
        "npm",
        "echo \"$PWD $*\" > args; rm -rf .npm/_cacache",
    );
    let e = env(&home, true);
    let found = caches::list(&e)
        .into_iter()
        .find(|f| f.cache.id == "npm")
        .unwrap();
    assert_eq!(
        found.program.as_deref(),
        Some(home.join("bin/npm").as_path())
    );
    let ran = caches::run(&found, &e, Duration::from_secs(30)).unwrap();
    assert_eq!(ran.status, Some(0), "{}", ran.stderr);
    assert!(!home.join(".npm/_cacache").exists());
    let args = fs::read_to_string(home.join("args")).unwrap();
    assert_eq!(
        args.trim(),
        format!("{} cache clean --force", home.display())
    );
}

#[test]
fn a_failing_or_hanging_tool_is_reported() {
    let tmp = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(tmp.path()).unwrap();
    fs::create_dir_all(home.join(".cache/uv")).unwrap();
    fs::create_dir_all(home.join("Library/Caches/deno")).unwrap();
    tool(&home, "uv", "echo 'no permission' >&2; exit 3");
    tool(&home, "deno", "sleep 30");
    let e = env(&home, true);
    let found = caches::list(&e);
    let uv = found.iter().find(|f| f.cache.id == "uv").unwrap();
    let ran = caches::run(uv, &e, Duration::from_secs(30)).unwrap();
    assert_eq!(ran.status, Some(3));
    assert_eq!(ran.stderr, "no permission");
    let deno = found.iter().find(|f| f.cache.id == "deno").unwrap();
    let ran = caches::run(deno, &e, Duration::from_millis(300)).unwrap();
    assert_eq!(ran.status, None, "stopped at the time limit");
}

#[test]
fn a_cache_without_a_command_cannot_be_run() {
    let tmp = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(tmp.path()).unwrap();
    fs::create_dir_all(home.join(".gradle/caches")).unwrap();
    let e = env(&home, true);
    let gradle = caches::list(&e)
        .into_iter()
        .find(|f| f.cache.id == "gradle")
        .unwrap();
    assert!(gradle.cache.command.is_empty() && gradle.program.is_none());
    assert!(caches::run(&gradle, &e, Duration::from_secs(1)).is_err());
}

/// I20: Xcode archives, simulators and Xcode's own caches are measured only.
/// Archives hold the dSYMs that crash reports from shipped versions need, so
/// AgentClean never runs anything on them; they are macOS only.
#[test]
fn xcode_archives_and_simulators_are_measured_only() {
    let ids = [
        "xcode-archives",
        "simulator-caches",
        "xcode-previews",
        "xcode-cache",
    ];
    let mac = caches::known(&env(Path::new("/Users/dev"), true));
    for id in ids {
        let c = mac
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("{id} missing"));
        assert!(c.command.is_empty(), "{id} must be measured only");
        assert!(
            c.path.starts_with("/Users/dev/Library"),
            "{id} at {}",
            c.path.display()
        );
    }
    let linux = caches::known(&env(Path::new("/home/dev"), false));
    assert!(linux.iter().all(|c| !ids.contains(&c.id)));
}

/// The only simulator command is the one that removes simulators whose iOS
/// is no longer installed; never `delete all` or `erase`.
#[test]
fn simulators_are_cleared_only_of_unavailable_ones() {
    let mac = caches::known(&env(Path::new("/Users/dev"), true));
    let sims = mac.iter().find(|c| c.id == "simulator-devices").unwrap();
    assert_eq!(sims.command, ["xcrun", "simctl", "delete", "unavailable"]);
}

/// F18: editor caches, test browsers, Docker's disk and AI model stores are
/// shown with their size and never cleared by AgentClean.
#[test]
fn editor_browser_docker_and_model_stores_are_measured_only() {
    let mac = caches::known(&env(Path::new("/Users/dev"), true));
    let linux = caches::known(&env(Path::new("/home/dev"), false));
    let both = [
        "vscode-cache",
        "vscode-cached-data",
        "vscode-extension-downloads",
        "cursor-cache",
        "cursor-cached-data",
        "playwright-browsers",
        "cypress-binaries",
        "huggingface",
        "torch-hub",
        "ollama-models",
        "lmstudio-models",
    ];
    for id in both {
        for (list, home) in [(&mac, "/Users/dev"), (&linux, "/home/dev")] {
            let c = list
                .iter()
                .find(|c| c.id == id)
                .unwrap_or_else(|| panic!("{id} missing under {home}"));
            assert!(c.command.is_empty(), "{id} must be measured only");
            assert!(c.path.starts_with(home), "{id} at {}", c.path.display());
        }
    }
    for id in ["docker-desktop", "jetbrains"] {
        let c = mac.iter().find(|c| c.id == id).unwrap();
        assert!(c.command.is_empty(), "{id} must be measured only");
    }
    let hf = mac.iter().find(|c| c.id == "huggingface").unwrap();
    assert_eq!(hf.path, Path::new("/Users/dev/.cache/huggingface"));
}
