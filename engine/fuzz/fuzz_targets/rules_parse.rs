//! `rules::parse` on arbitrary text: never panics, and every rule it accepts
//! names only plain relative paths that stay inside the project.
#![no_main]

use std::path::{Component, Path};

use agentclean_core::deny::protected_target;
use agentclean_core::rules;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(rule) = rules::parse("fuzz", text) else {
        return;
    };
    assert!(!rule.detect.any_files.is_empty());
    for marker in &rule.detect.any_files {
        assert!(!marker.is_empty() && !marker.contains('/'), "{marker:?}");
    }
    for a in &rule.artifacts {
        let parts: Vec<_> = Path::new(&a.path).components().collect();
        assert!((1..=4).contains(&parts.len()), "{:?}", a.path);
        assert!(
            parts.iter().all(|c| matches!(c, Component::Normal(_))),
            "{:?} escapes the project",
            a.path
        );
        let last = Path::new(&a.path).file_name().and_then(|n| n.to_str());
        assert!(
            last.is_some_and(|n| protected_target(n).is_none()),
            "{:?}",
            a.path
        );
    }
});
