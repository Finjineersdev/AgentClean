//! Names that are never deletable and never valid in a rule.
//!
//! Matching is on a single path component, case-insensitively, because the
//! default macOS file system is case-insensitive.

/// Names that conventionally hold source. A directory with one of these
/// names is never a deletion target, but may sit above one
/// (`android/app/build` is build output).
const SOURCE_NAMES: &[&str] = &[
    "src", "source", "sources", "lib", "app", "apps", "packages", "test", "tests", "docs",
];

/// Names that hold history or identity. Nothing at or below one is deletable.
const PROTECTED_EXACT: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    ".jj",
    ".ssh",
    ".gnupg",
    ".aws",
    ".kube",
    ".docker",
    "id_rsa",
    "id_dsa",
    "id_ecdsa",
    "id_ed25519",
    "credentials.json",
    "google-services.json",
    "googleservice-info.plist",
    "keystore",
    "secrets",
];

/// Lockfiles: needed to rebuild exactly what was removed.
const LOCKFILES: &[&str] = &[
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "bun.lockb",
    "bun.lock",
    "cargo.lock",
    "podfile.lock",
    "gemfile.lock",
    "composer.lock",
    "poetry.lock",
    "uv.lock",
    "pubspec.lock",
    "go.sum",
    "package.resolved",
];

/// Suffixes of key, certificate and signing material.
const PROTECTED_SUFFIXES: &[&str] = &[
    ".pem",
    ".key",
    ".p12",
    ".pfx",
    ".jks",
    ".keystore",
    ".mobileprovision",
    ".provisionprofile",
    ".cer",
    ".crt",
    ".asc",
    ".gpg",
];

/// Returns the reason `name` may not be the final component of a deletion
/// target, if there is one. Stricter than [`protected_component`].
pub fn protected_target(name: &str) -> Option<&'static str> {
    if SOURCE_NAMES.contains(&name.to_lowercase().as_str()) {
        return Some("conventionally holds source");
    }
    protected_component(name)
}

/// Returns the reason a path component is protected wherever it appears in a
/// path, if it is.
pub fn protected_component(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    if PROTECTED_EXACT.contains(&lower.as_str()) {
        return Some("holds history or credentials");
    }
    if LOCKFILES.contains(&lower.as_str()) {
        return Some("lockfile");
    }
    if lower == ".env" || lower.starts_with(".env.") {
        return Some("environment file");
    }
    if PROTECTED_SUFFIXES.iter().any(|s| lower.ends_with(s)) {
        return Some("key or signing material");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{protected_component, protected_target};

    #[test]
    fn env_files_in_every_spelling() {
        for name in [".env", ".env.local", ".ENV.production", ".env.staging"] {
            assert!(protected_component(name).is_some(), "{name}");
        }
        assert!(protected_component(".envrc-cache").is_none());
    }

    #[test]
    fn keys_and_lockfiles() {
        for name in [
            "id_ed25519",
            "release.jks",
            "Cargo.lock",
            "dist.mobileprovision",
            ".git",
        ] {
            assert!(protected_component(name).is_some(), "{name}");
        }
    }

    #[test]
    fn source_names_block_targets_but_not_parents() {
        for name in ["src", "App", "lib", "tests"] {
            assert!(protected_target(name).is_some(), "{name}");
            assert!(protected_component(name).is_none(), "{name}");
        }
        assert!(protected_target(".git").is_some());
    }

    #[test]
    fn ordinary_artifacts_are_not_protected() {
        for name in ["node_modules", "target", ".next", ".venv", "DerivedData"] {
            assert!(protected_target(name).is_none(), "{name}");
        }
    }
}
