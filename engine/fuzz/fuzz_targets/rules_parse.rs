//! `rules::parse` on arbitrary text: never panics, and every rule it accepts
//! names only plain relative paths that stay inside the project, plain
//! markers, one-`*` globs, and evidence that cannot reach outside the project
//! or into protected files.
#![no_main]

use std::path::{Component, Path};

use opensweep_core::deny::{protected_component, protected_target};
use opensweep_core::rules;
use libfuzzer_sys::fuzz_target;

fn plain_name(name: &str) {
    assert!(
        !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', '*']),
        "{name:?}"
    );
}

fn glob(pattern: &str) {
    assert_eq!(pattern.matches('*').count(), 1, "{pattern:?}");
    assert!(
        pattern.len() > 1 && !pattern.contains(['/', '\\']),
        "{pattern:?}"
    );
}

/// Relative, 1 to 4 plain components, none of them protected.
fn inside_project(path: &str, protected: fn(&str) -> Option<&'static str>) {
    assert!(
        !path.split('/').any(|s| s.is_empty() || s == "."),
        "{path:?}"
    );
    let parts: Vec<_> = Path::new(path).components().collect();
    assert!((1..=4).contains(&parts.len()), "{path:?}");
    for c in parts {
        let Component::Normal(os) = c else {
            panic!("{path:?} escapes the project");
        };
        let name = os.to_str().unwrap_or_default();
        assert!(protected(name).is_none(), "{path:?}");
    }
}

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(rule) = rules::parse("fuzz", text) else {
        return;
    };
    let d = &rule.detect;
    assert!(!(d.any_files.is_empty() && d.any_glob.is_empty() && d.any_dirs.is_empty()));
    d.any_files
        .iter()
        .chain(&d.any_dirs)
        .for_each(|m| plain_name(m));
    d.any_glob.iter().for_each(|g| glob(g));
    if let Some(x) = &d.exclude_if {
        x.parent_any_files.iter().for_each(|m| plain_name(m));
    }
    if rule.schema == 1 {
        assert!(d.any_glob.is_empty() && d.any_dirs.is_empty() && d.exclude_if.is_none());
    }
    for a in &rule.artifacts {
        inside_project(&a.path, protected_component);
        let last = Path::new(&a.path).file_name().and_then(|n| n.to_str());
        assert!(
            last.is_some_and(|n| protected_target(n).is_none()),
            "{:?}",
            a.path
        );
        let Some(r) = &a.requires else { continue };
        assert!(rule.schema >= 2);
        r.any_files
            .iter()
            .for_each(|f| inside_project(f, protected_component));
        r.any_glob.iter().for_each(|g| glob(g));
        for f in &r.inside {
            plain_name(f);
            assert!(protected_component(f).is_none(), "{f:?}");
        }
        if let Some(j) = &r.json {
            inside_project(&j.file, protected_component);
            assert!(!j.key.split('.').any(str::is_empty), "{:?}", j.key);
        }
    }
});
