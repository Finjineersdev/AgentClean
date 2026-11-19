//! The manifest loader on arbitrary bytes: never panics, and every accepted
//! entry points at a single item directly under the run's `items/` folder,
//! which is the only place restore reads from and purge deletes.
#![no_main]

use std::path::Path;

use agentclean_core::quarantine::{parse_manifest, Manifest};
use libfuzzer_sys::fuzz_target;

/// The seed's run id, so mutations of it reach the entry checks.
const RUN_ID: &str = "20261119T094828Z-42";

fuzz_target!(|data: &[u8]| {
    // `Store::load` reads with `read_to_string`, so non-UTF-8 never gets further.
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(manifest) = parse_manifest(RUN_ID, text) else {
        return;
    };
    let items = Path::new("/store/runs").join(RUN_ID).join("items");
    for e in &manifest.entries {
        let item = items.join(&e.stored);
        assert_eq!(
            item.parent(),
            Some(items.as_path()),
            "{:?} escapes",
            e.stored
        );
        assert!(e.original.is_absolute());
        assert!(e.original.starts_with(&e.project_root) && e.original != e.project_root);
        assert!(!e
            .original
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir)));
    }
    // What we accept, we can write back and read again unchanged.
    let json = serde_json::to_string(&manifest).unwrap_or_default();
    let again: Option<Manifest> = parse_manifest(RUN_ID, &json).ok();
    assert_eq!(again.as_ref(), Some(&manifest));
});
