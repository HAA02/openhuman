//! Skill manifest verification.
//!
//! A "manifest" here is the SKILL.md (or any text file) that ships with a
//! skill. Verification:
//!   1. The file must be readable.
//!   2. It must parse as text (UTF-8) and be non-empty.
//!   3. It must contain a YAML frontmatter block delimited by `---` on its
//!      own lines.
//!   4. The frontmatter must declare a non-empty `name`.
//!
//! The verification result includes the SHA-256 of the on-disk bytes. Callers
//! pin this hash when installing; subsequent `verify` calls that yield a
//! different SHA-256 indicate tampering and yield [`ManifestVerdict::Invalid`]
//! when compared against the pinned value.
//!
//! Pinning is left to the caller (e.g. the install flow can persist
//! `<skill_dir>/manifest.sha256`). This module's responsibility is to
//! deterministically produce the hash + issue list for a given file.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Outcome of [`verify_manifest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestVerdict {
    Valid,
    Invalid,
}

/// Full verification report. `sha256` is always computed when the file is
/// readable; absent (`String::new()`) only on read failure.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ManifestVerification {
    pub verdict: ManifestVerdict,
    pub sha256: String,
    pub path: PathBuf,
    pub issues: Vec<String>,
}

/// Verify a SKILL.md manifest at `path`. If `expected_sha256` is `Some`, the
/// returned verdict additionally enforces equality against the on-disk hash
/// (PRD §1.3 FR-05 integrity check).
pub fn verify_manifest(path: &Path, expected_sha256: Option<&str>) -> ManifestVerification {
    let mut issues = Vec::new();
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            issues.push(format!("read failed: {e}"));
            return ManifestVerification {
                verdict: ManifestVerdict::Invalid,
                sha256: String::new(),
                path: path.to_path_buf(),
                issues,
            };
        }
    };
    let sha256 = sha256_hex(&bytes);

    if bytes.is_empty() {
        issues.push("manifest is empty".to_string());
    }

    let body = String::from_utf8(bytes).unwrap_or_else(|err| {
        issues.push(format!("non-utf8 content: {err}"));
        String::new()
    });
    if !body.is_empty() {
        validate_frontmatter(&body, &mut issues);
    }

    if let Some(expected) = expected_sha256 {
        if !sha_eq(expected, &sha256) {
            issues.push(format!(
                "sha256 mismatch: expected {} got {}",
                expected, sha256
            ));
        }
    }

    let verdict = if issues.is_empty() {
        ManifestVerdict::Valid
    } else {
        ManifestVerdict::Invalid
    };
    ManifestVerification {
        verdict,
        sha256,
        path: path.to_path_buf(),
        issues,
    }
}

/// SHA-256 of a byte slice, returned as 64-char lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn sha_eq(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn validate_frontmatter(body: &str, issues: &mut Vec<String>) {
    let trimmed = body.trim_start();
    if !trimmed.starts_with("---") {
        issues.push("missing YAML frontmatter (expected leading `---`)".to_string());
        return;
    }
    // Find the closing `---` on its own line.
    let after_open = match trimmed.find('\n') {
        Some(idx) => &trimmed[idx + 1..],
        None => {
            issues.push("frontmatter has no body after opening `---`".to_string());
            return;
        }
    };
    let close_idx = after_open
        .lines()
        .scan(0usize, |acc, line| {
            let start = *acc;
            *acc += line.len() + 1;
            Some((start, line))
        })
        .find_map(|(start, line)| (line.trim() == "---").then_some(start));
    let Some(close) = close_idx else {
        issues.push("frontmatter missing closing `---`".to_string());
        return;
    };
    let yaml = &after_open[..close];
    let parsed: Result<serde_yaml::Value, _> = serde_yaml::from_str(yaml);
    match parsed {
        Err(e) => issues.push(format!("frontmatter YAML invalid: {e}")),
        Ok(serde_yaml::Value::Mapping(map)) => {
            let name = map
                .get(serde_yaml::Value::String("name".into()))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .unwrap_or("");
            if name.is_empty() {
                issues.push("frontmatter missing required field `name`".to_string());
            }
        }
        Ok(_) => issues.push("frontmatter root must be a YAML mapping".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write(dir: &TempDir, body: &str) -> PathBuf {
        let path = dir.path().join("SKILL.md");
        fs::write(&path, body).expect("write fixture");
        path
    }

    const GOOD: &str = "---\nname: demo\ndescription: example\n---\n# Body\n";

    #[test]
    fn accepts_well_formed_manifest() {
        let dir = TempDir::new().unwrap();
        let path = write(&dir, GOOD);
        let v = verify_manifest(&path, None);
        assert_eq!(v.verdict, ManifestVerdict::Valid, "issues: {:?}", v.issues);
        assert_eq!(v.sha256.len(), 64);
    }

    #[test]
    fn rejects_missing_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("missing.md");
        let v = verify_manifest(&path, None);
        assert_eq!(v.verdict, ManifestVerdict::Invalid);
        assert!(v.issues.iter().any(|i| i.contains("read failed")));
    }

    #[test]
    fn rejects_empty_file() {
        let dir = TempDir::new().unwrap();
        let path = write(&dir, "");
        let v = verify_manifest(&path, None);
        assert_eq!(v.verdict, ManifestVerdict::Invalid);
        assert!(v.issues.iter().any(|i| i.contains("empty")));
    }

    #[test]
    fn rejects_missing_frontmatter() {
        let dir = TempDir::new().unwrap();
        let path = write(&dir, "# No frontmatter here\nbody\n");
        let v = verify_manifest(&path, None);
        assert_eq!(v.verdict, ManifestVerdict::Invalid);
        assert!(v.issues.iter().any(|i| i.contains("frontmatter")));
    }

    #[test]
    fn rejects_missing_name() {
        let dir = TempDir::new().unwrap();
        let path = write(&dir, "---\ndescription: anonymous\n---\n");
        let v = verify_manifest(&path, None);
        assert_eq!(v.verdict, ManifestVerdict::Invalid);
        assert!(v.issues.iter().any(|i| i.contains("name")));
    }

    #[test]
    fn rejects_tampered_manifest_when_expected_hash_provided() {
        let dir = TempDir::new().unwrap();
        let path = write(&dir, GOOD);
        let baseline = verify_manifest(&path, None);
        assert_eq!(baseline.verdict, ManifestVerdict::Valid);
        // Tamper: rewrite with a different body.
        fs::write(
            &path,
            "---\nname: demo\ndescription: tampered\n---\nNew body\n",
        )
        .unwrap();
        let v = verify_manifest(&path, Some(&baseline.sha256));
        assert_eq!(v.verdict, ManifestVerdict::Invalid);
        assert!(v.issues.iter().any(|i| i.contains("sha256 mismatch")));
        assert_ne!(v.sha256, baseline.sha256);
    }

    #[test]
    fn matching_hash_is_valid() {
        let dir = TempDir::new().unwrap();
        let path = write(&dir, GOOD);
        let baseline = verify_manifest(&path, None);
        let v = verify_manifest(&path, Some(&baseline.sha256));
        assert_eq!(v.verdict, ManifestVerdict::Valid);
        assert!(v.issues.is_empty());
    }

    #[test]
    fn sha256_is_deterministic_and_lowercase() {
        let h = sha256_hex(b"hello");
        assert_eq!(h.len(), 64);
        assert!(h
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_eq!(sha256_hex(b"hello"), h);
    }
}
