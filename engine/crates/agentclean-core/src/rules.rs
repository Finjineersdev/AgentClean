//! The rule catalogue: one TOML file per ecosystem.
//!
//! A rule file says how to recognise a project and which paths under its root
//! are rebuildable. Rules are data, reviewed like code, and linted here so a
//! bad rule fails CI instead of deleting someone's work.
//! The format is specified in `docs/RULE-SCHEMA.md`.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::deny::{protected_component, protected_target};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Class {
    /// Recreated by the next build with no user action.
    Safe,
    /// Needs a reinstall or download before the project works again.
    Caution,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Detect {
    /// The project root must contain at least one of these files.
    pub any_files: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    /// Path relative to the project root.
    pub path: String,
    pub class: Class,
    /// Shown to the user: what this is and why removing it is recoverable.
    pub reason: String,
    /// Shown to the user: how to get it back.
    pub restore: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleFile {
    pub schema: u32,
    pub id: String,
    pub name: String,
    pub detect: Detect,
    #[serde(rename = "artifact")]
    pub artifacts: Vec<Artifact>,
}

/// A path a rule says may be rebuildable in one project. The only way to
/// obtain one is from a linted [`RuleFile`] whose marker file is present, and
/// the safety guard accepts nothing else: there is no free-form delete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    rule_id: String,
    rel: PathBuf,
    class: Class,
}

impl Candidate {
    pub fn rule_id(&self) -> &str {
        &self.rule_id
    }

    /// Path relative to the project root.
    pub fn rel(&self) -> &Path {
        &self.rel
    }

    pub fn class(&self) -> Class {
        self.class
    }

    #[cfg(test)]
    pub(crate) fn unchecked(rel: &str) -> Self {
        Self {
            rule_id: "test".to_owned(),
            rel: PathBuf::from(rel),
            class: Class::Safe,
        }
    }
}

impl RuleFile {
    /// True if `project_root` directly contains one of this rule's marker files.
    pub fn detects(&self, project_root: &Path) -> bool {
        self.detect.any_files.iter().any(|marker| {
            std::fs::symlink_metadata(project_root.join(marker)).is_ok_and(|m| m.is_file())
        })
    }

    /// Candidates for `project_root`, or none if the rule does not detect it.
    /// A candidate is a claim by a rule, not permission: the guard decides.
    pub fn candidates(&self, project_root: &Path) -> Vec<Candidate> {
        if !self.detects(project_root) {
            return Vec::new();
        }
        self.artifacts
            .iter()
            .map(|a| Candidate {
                rule_id: self.id.clone(),
                rel: PathBuf::from(&a.path),
                class: a.class,
            })
            .collect()
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RuleError {
    #[error("{file}: {message}")]
    Parse { file: String, message: String },
    #[error("{file}: unsupported schema {found}, expected {SCHEMA_VERSION}")]
    Schema { file: String, found: u32 },
    #[error("{file}: {message}")]
    Lint { file: String, message: String },
    #[error("{file}: cannot read: {message}")]
    Io { file: String, message: String },
}

/// Parses and lints one rule file. `file` is used in messages only.
pub fn parse(file: &str, text: &str) -> Result<RuleFile, RuleError> {
    let rule: RuleFile = toml::from_str(text).map_err(|e| RuleError::Parse {
        file: file.to_owned(),
        message: e.message().to_owned(),
    })?;
    if rule.schema != SCHEMA_VERSION {
        return Err(RuleError::Schema {
            file: file.to_owned(),
            found: rule.schema,
        });
    }
    lint(file, &rule)?;
    Ok(rule)
}

fn lint_err(file: &str, message: String) -> RuleError {
    RuleError::Lint {
        file: file.to_owned(),
        message,
    }
}

fn lint(file: &str, rule: &RuleFile) -> Result<(), RuleError> {
    let id_ok = !rule.id.is_empty()
        && rule
            .id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !id_ok {
        return Err(lint_err(
            file,
            format!("id {:?} must be lowercase letters, digits and '-'", rule.id),
        ));
    }
    if rule.name.trim().is_empty() {
        return Err(lint_err(file, "name is empty".to_owned()));
    }
    if rule.detect.any_files.is_empty() {
        return Err(lint_err(
            file,
            "detect.any_files is empty: a rule must require a marker file".to_owned(),
        ));
    }
    for marker in &rule.detect.any_files {
        if marker.is_empty() || marker.contains('/') || marker.contains('\\') {
            return Err(lint_err(
                file,
                format!("marker {marker:?} must be a plain file name"),
            ));
        }
    }
    if rule.artifacts.is_empty() {
        return Err(lint_err(file, "no [[artifact]] entries".to_owned()));
    }
    let mut seen = BTreeSet::new();
    for a in &rule.artifacts {
        lint_artifact_path(file, &a.path)?;
        if !seen.insert(a.path.to_lowercase()) {
            return Err(lint_err(
                file,
                format!("artifact {:?} listed twice", a.path),
            ));
        }
        if a.reason.trim().is_empty() || a.restore.trim().is_empty() {
            return Err(lint_err(
                file,
                format!("artifact {:?} needs a reason and a restore hint", a.path),
            ));
        }
    }
    Ok(())
}

/// An artifact path must be a plain relative path that cannot reach outside
/// the project or name anything protected.
fn lint_artifact_path(file: &str, path: &str) -> Result<(), RuleError> {
    let bad = |why: &str| Err(lint_err(file, format!("artifact {path:?}: {why}")));
    if path.is_empty() || path.trim() != path {
        return bad("empty or padded with whitespace");
    }
    if path.contains('\\') || path.contains('\0') {
        return bad("contains a backslash or NUL");
    }
    if path
        .chars()
        .any(|c| matches!(c, '*' | '?' | '[' | ']' | '{' | '}' | '~' | '$'))
    {
        return bad("wildcards and expansions are not allowed");
    }
    if path.ends_with('/') {
        return bad("must not end with '/'");
    }
    let p = Path::new(path);
    let last = p.file_name();
    let mut count = 0usize;
    for c in p.components() {
        match c {
            Component::Normal(os) => {
                let Some(name) = os.to_str() else {
                    return bad("not valid UTF-8");
                };
                let verdict = if Some(os) == last {
                    protected_target(name)
                } else {
                    protected_component(name)
                };
                if let Some(why) = verdict {
                    return Err(lint_err(
                        file,
                        format!("artifact {path:?}: component {name:?} is protected ({why})"),
                    ));
                }
                count += 1;
            }
            _ => return bad("must be relative with no '.', '..' or root components"),
        }
    }
    if count == 0 {
        return bad("names the project root itself");
    }
    if count > 4 {
        return bad("deeper than 4 components");
    }
    Ok(())
}

/// Loads every `*.toml` file in `dir`, in name order, and checks ids are unique.
pub fn load_dir(dir: &Path) -> Result<Vec<RuleFile>, RuleError> {
    let io = |e: std::io::Error| RuleError::Io {
        file: dir.display().to_string(),
        message: e.to_string(),
    };
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .map_err(io)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    files.sort();
    let mut ids = BTreeSet::new();
    let mut out = Vec::with_capacity(files.len());
    for path in files {
        let label = path.display().to_string();
        let text = std::fs::read_to_string(&path).map_err(|e| RuleError::Io {
            file: label.clone(),
            message: e.to_string(),
        })?;
        let rule = parse(&label, &text)?;
        if !ids.insert(rule.id.clone()) {
            return Err(lint_err(&label, format!("duplicate rule id {:?}", rule.id)));
        }
        out.push(rule);
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    fn rule_with(path: &str) -> String {
        format!(
            "schema = 1\nid = \"x\"\nname = \"X\"\n[detect]\nany_files = [\"x.json\"]\n\
             [[artifact]]\npath = {path:?}\nclass = \"safe\"\nreason = \"r\"\nrestore = \"s\"\n"
        )
    }

    #[test]
    fn accepts_plain_artifacts() {
        for p in ["node_modules", "target", "android/app/build", ".next"] {
            assert!(parse("t", &rule_with(p)).is_ok(), "{p}");
        }
    }

    #[test]
    fn rejects_dangerous_artifacts() {
        for p in [
            "",
            ".",
            "..",
            "../x",
            "/etc",
            "a/../b",
            "src",
            "SRC",
            ".git",
            "x/.git/y",
            ".env",
            ".env.local",
            "*",
            "build*",
            "~/x",
            "$HOME",
            "a\\b",
            "x/",
            " x",
            "keys/prod.pem",
            "Cargo.lock",
            "a/b/c/d/e",
        ] {
            assert!(
                matches!(parse("t", &rule_with(p)), Err(RuleError::Lint { .. })),
                "{p:?} was accepted"
            );
        }
    }

    #[test]
    fn rejects_unknown_fields_and_wrong_schema() {
        let extra = rule_with("target") + "delete_everything = true\n";
        assert!(matches!(parse("t", &extra), Err(RuleError::Parse { .. })));
        let v2 = rule_with("target").replace("schema = 1", "schema = 2");
        assert_eq!(
            parse("t", &v2).err(),
            Some(RuleError::Schema {
                file: "t".into(),
                found: 2
            })
        );
    }

    #[test]
    fn rejects_rule_without_marker() {
        let none = rule_with("target").replace("[\"x.json\"]", "[]");
        assert!(matches!(parse("t", &none), Err(RuleError::Lint { .. })));
    }

    #[test]
    fn candidates_need_the_marker_file() {
        let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let rule = parse("t", &rule_with("target")).unwrap_or_else(|e| panic!("{e}"));
        assert!(rule.candidates(tmp.path()).is_empty());
        std::fs::write(tmp.path().join("x.json"), "{}").unwrap_or_else(|e| panic!("{e}"));
        let found = rule.candidates(tmp.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].rel(), Path::new("target"));
        assert_eq!(found[0].rule_id(), "x");
    }

    #[test]
    fn bundled_rules_are_valid() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules");
        let rules = load_dir(&dir);
        assert!(rules.is_ok(), "{rules:?}");
        assert!(rules.is_ok_and(|r| r.len() >= 3));
    }
}
