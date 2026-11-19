#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

use crate::discover::{discover, Event, Options, Skip};
use crate::rules::{load_dir, RuleFile};

fn rules() -> Vec<RuleFile> {
    load_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules")).unwrap()
}

/// Creates each file (and its folders); a path ending in `/` is a folder.
fn tree(base: &Path, paths: &[&str]) {
    for p in paths {
        let full = base.join(p);
        if p.ends_with('/') {
            fs::create_dir_all(full).unwrap();
        } else {
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(full, "{}").unwrap();
        }
    }
}

fn base() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(tmp.path()).unwrap();
    (tmp, base)
}

fn run(opts: &Options) -> Vec<Event> {
    let mut events = Vec::new();
    discover(&rules(), opts, |e| events.push(e));
    events
}

fn rel(base: &Path, p: &Path) -> String {
    p.strip_prefix(base).unwrap().display().to_string()
}

/// Project roots relative to `base`, in the order reported.
fn projects(base: &Path, events: &[Event]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Project { root, .. } => Some(rel(base, root)),
            Event::Skipped { .. } => None,
        })
        .collect()
}

/// Every candidate as a path relative to `base`.
fn candidates(base: &Path, events: &[Event]) -> Vec<String> {
    let mut out = Vec::new();
    for e in events {
        if let Event::Project {
            root, candidates, ..
        } = e
        {
            out.extend(candidates.iter().map(|c| rel(base, &root.join(c.rel()))));
        }
    }
    out
}

fn skipped(base: &Path, events: &[Event]) -> Vec<(String, Skip)> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Skipped { path, reason } => Some((rel(base, path), reason.clone())),
            Event::Project { .. } => None,
        })
        .collect()
}

#[test]
fn finds_every_project_in_a_layout_like_the_owners() {
    let (_tmp, home) = base();
    tree(
        &home,
        &[
            "dev/api/Cargo.toml",
            "dev/api/target/debug/",
            "development/shop/package.json",
            "development/shop/.next/",
            "development/shop/node_modules/left-pad/package.json",
            "Mobile Projects/app/pubspec.yaml",
            "Mobile Projects/app/.metadata",
            "Mobile Projects/app/build/",
            "Mobile Projects/app/android/settings.gradle",
            "Mobile Projects/app/android/app/src/main/AndroidManifest.xml",
            "Mobile Projects/app/ios/Runner.xcodeproj/project.pbxproj",
            "Mobile Projects/app/ios/Podfile",
            "Mobile Projects/app/ios/Pods/",
            "Mobile Projects/app/android/app/build.gradle",
            "Mobile Projects/app/ios/Flutter/ephemeral/Packages/Gen/Package.swift",
            "Mobile Projects/app/backend/api/package.json",
            "Mobile Projects/plugin/pubspec.yaml",
            "Mobile Projects/plugin/example/pubspec.yaml",
            "Mobile Projects/plugin/example/android/build.gradle",
            "Mobile Projects/plugin/example/android/app/build.gradle",
            "Mobile Projects/kotlin/settings.gradle.kts",
            "Mobile Projects/kotlin/app/build.gradle.kts",
            "Mobile Projects/kotlin/app/src/main/AndroidManifest.xml",
            "Mobile Projects/kotlin/app/build/",
            "Laravel Projects/blog/composer.json",
            "Laravel Projects/blog/vendor/autoload.php",
            "Laravel Projects/blog/vendor/laravel/framework/composer.json",
            "Laravel Projects/blog/storage/app/",
            "pythonprojects/bot/requirements.txt",
            "pythonprojects/bot/.venv/pyvenv.cfg",
            "websites/mono/package.json",
            "websites/mono/node_modules/react/package.json",
            "websites/mono/packages/web/package.json",
            "websites/mono/packages/web/vite.config.ts",
            "websites/mono/packages/web/dist/",
            "websites/mono/packages/api/package.json",
            "websites/notes/readme.md",
            // Never scanned: home skips, hidden tool caches, bundles.
            "Library/Developer/thing/package.json",
            "go/pkg/mod/example.com/m/go.mod",
            ".cargo/registry/src/crate/Cargo.toml",
            "Desktop/Tool.app/Contents/package.json",
        ],
    );
    let mut opts = Options::new(vec![home.clone()]);
    opts.home = Some(home.clone());
    let events = run(&opts);
    assert_eq!(
        projects(&home, &events),
        [
            "Laravel Projects/blog",
            "Mobile Projects/app",
            "Mobile Projects/app/backend/api",
            "Mobile Projects/kotlin",
            "Mobile Projects/kotlin/app",
            "Mobile Projects/plugin",
            "Mobile Projects/plugin/example",
            "dev/api",
            "development/shop",
            "pythonprojects/bot",
            "websites/mono",
            "websites/mono/packages/api",
            "websites/mono/packages/web",
        ]
    );
    let mut got = candidates(&home, &events);
    got.sort();
    assert_eq!(
        got,
        [
            "Laravel Projects/blog/vendor",
            "Mobile Projects/app/build",
            "Mobile Projects/app/ios/Pods",
            "Mobile Projects/kotlin/app/build",
            "dev/api/target",
            "development/shop/.next",
            "development/shop/node_modules",
            "pythonprojects/bot/.venv",
            "websites/mono/node_modules",
            "websites/mono/packages/web/dist",
        ]
    );
    assert!(skipped(&home, &events).is_empty(), "{events:?}");
}

#[test]
fn overlapping_roots_report_each_project_once() {
    let (_tmp, base) = base();
    tree(&base, &["dev/api/Cargo.toml"]);
    let events = run(&Options::new(vec![
        base.clone(),
        base.join("dev"),
        base.join("dev/api"),
    ]));
    assert_eq!(projects(&base, &events), ["dev/api"]);
}

#[cfg(unix)]
#[test]
fn links_are_never_followed() {
    let (_tmp, base) = base();
    let (_out, outside) = self::base();
    tree(&base, &["root/"]);
    tree(&outside, &["proj/package.json", "proj/node_modules/"]);
    std::os::unix::fs::symlink(&base, base.join("root/loop")).unwrap();
    std::os::unix::fs::symlink(outside.join("proj"), base.join("root/proj")).unwrap();
    let events = run(&Options::new(vec![base.join("root")]));
    assert!(events.is_empty(), "{events:?}");
}

#[cfg(unix)]
#[test]
fn other_volumes_are_not_entered() {
    // /dev is its own file system on macOS (devfs) and Linux (devtmpfs).
    let mut opts = Options::new(vec![PathBuf::from("/")]);
    opts.max_depth = 1;
    let mut events = Vec::new();
    discover(&[], &opts, |e| events.push(e));
    let dev = Event::Skipped {
        path: PathBuf::from("/dev"),
        reason: Skip::OtherVolume,
    };
    assert!(events.contains(&dev), "{events:?}");
}

#[test]
fn the_quarantine_store_is_not_entered() {
    let (_tmp, base) = base();
    tree(
        &base,
        &[
            "store/runs/1/items/0/react/package.json",
            "store/runs/1/items/0/react/node_modules/",
        ],
    );
    let mut opts = Options::new(vec![base.clone()]);
    assert_eq!(projects(&base, &run(&opts)), ["store/runs/1/items/0/react"]);
    opts.store = Some(base.join("store"));
    assert!(run(&opts).is_empty());
}

#[test]
fn bundles_and_hidden_folders_are_not_entered() {
    let (_tmp, base) = base();
    tree(
        &base,
        &[
            "App.app/Contents/package.json",
            "ios/Tool.xcodeproj/x/package.json",
            "ios/Tool.xcworkspace/x/package.json",
            ".hidden/proj/package.json",
        ],
    );
    // ios is an Xcode project (its marker is the bundle); nothing inside is.
    let events = run(&Options::new(vec![base.clone()]));
    assert_eq!(projects(&base, &events), ["ios"]);
}

#[test]
fn a_hidden_root_is_scanned() {
    let (_tmp, base) = base();
    tree(&base, &[".config/tool/package.json"]);
    let events = run(&Options::new(vec![base.join(".config")]));
    assert_eq!(projects(&base, &events), [".config/tool"]);
}

#[cfg(unix)]
#[test]
fn unreadable_folders_are_reported() {
    use std::os::unix::fs::PermissionsExt;
    let (_tmp, base) = base();
    tree(&base, &["locked/proj/package.json", "open/package.json"]);
    let locked = base.join("locked");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let readable = fs::read_dir(&locked).is_ok(); // root ignores permissions
    let events = run(&Options::new(vec![base.clone()]));
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(projects(&base, &events), ["open"]);
    if !readable {
        let s = skipped(&base, &events);
        assert_eq!(s.len(), 1, "{s:?}");
        assert_eq!(s[0].0, "locked");
        assert!(matches!(s[0].1, Skip::Unreadable(_)));
    }
}

#[test]
fn missing_roots_and_files_are_reported() {
    let (_tmp, base) = base();
    tree(&base, &["file.txt"]);
    let events = run(&Options::new(vec![
        base.join("gone"),
        base.join("file.txt"),
    ]));
    assert_eq!(events.len(), 2, "{events:?}");
    assert!(events.iter().all(|e| matches!(
        e,
        Event::Skipped {
            reason: Skip::Unreadable(_),
            ..
        }
    )));
}

#[test]
fn exclusions_and_limits_are_reported() {
    let (_tmp, base) = base();
    tree(&base, &["a/b/c/package.json", "private/package.json"]);

    let mut opts = Options::new(vec![base.clone()]);
    opts.exclude = vec![base.join("private")];
    opts.max_depth = 2;
    let events = run(&opts);
    assert!(projects(&base, &events).is_empty());
    assert_eq!(
        skipped(&base, &events),
        [
            ("private".to_owned(), Skip::Excluded),
            ("a/b/c".to_owned(), Skip::DepthLimit)
        ]
    );

    opts.max_depth = 3;
    assert_eq!(projects(&base, &run(&opts)), ["a/b/c"]);

    // Entries read: a, private (root), then b (in a): the limit stops at b.
    opts.max_entries = 2;
    let events = run(&opts);
    assert_eq!(
        skipped(&base, &events),
        [
            ("private".to_owned(), Skip::Excluded),
            ("a".to_owned(), Skip::EntryLimit)
        ]
    );
    opts.max_entries = 4;
    assert_eq!(projects(&base, &run(&opts)), ["a/b/c"]);
}
