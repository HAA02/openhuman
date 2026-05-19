//! PRD §3.1 Phase 4 Red tests (top-level integration of manifest + gate + ops).

use super::gate::{Decision, Permission, PermissionGate, SkillId};
use super::manifest::{verify_manifest, ManifestVerdict};
use super::ops::{permissions_registered_controllers, permissions_schemas};

#[test]
fn permissions_schemas_advertise_two_methods() {
    let names: Vec<_> = permissions_schemas()
        .into_iter()
        .map(|s| s.method_name())
        .collect();
    assert_eq!(
        names,
        vec![
            "security.verify_skill".to_string(),
            "security.list_permissions".to_string(),
        ]
    );
}

#[test]
fn permissions_registered_controllers_match_schema_count() {
    assert_eq!(
        permissions_registered_controllers().len(),
        permissions_schemas().len()
    );
}

#[test]
fn rejects_tampered_manifest() {
    use tempfile::TempDir;
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("SKILL.md");
    std::fs::write(&path, "---\nname: demo\n---\nbody\n").unwrap();
    let baseline = verify_manifest(&path, None);
    assert_eq!(baseline.verdict, ManifestVerdict::Valid);
    // Tamper.
    std::fs::write(&path, "---\nname: demo\n---\nDIFFERENT BODY\n").unwrap();
    let v = verify_manifest(&path, Some(&baseline.sha256));
    assert_eq!(v.verdict, ManifestVerdict::Invalid);
    assert!(v.issues.iter().any(|i| i.contains("sha256")));
}

#[test]
fn denies_file_write_without_permission() {
    let gate = PermissionGate::new();
    let skill = SkillId::test();
    assert_eq!(gate.check(&skill, Permission::FileWrite), Decision::Deny);
}

#[test]
fn allows_after_explicit_grant() {
    let gate = PermissionGate::new();
    let skill = SkillId::test();
    gate.grant(&skill, Permission::FileWrite);
    assert_eq!(gate.check(&skill, Permission::FileWrite), Decision::Allow);
    // Other permissions remain denied per deny-all default.
    for other in [
        Permission::FileRead,
        Permission::Network,
        Permission::Process,
        Permission::SystemInfo,
    ] {
        assert_eq!(gate.check(&skill, other), Decision::Deny, "{other:?}");
    }
}
