//! Runtime permission gating.
//!
//! `PermissionGate` holds a per-skill grant matrix. The default state is
//! **deny-all** (PRD §1.1 O2): if a skill has not been explicitly granted a
//! permission category, [`PermissionGate::check`] returns [`Decision::Deny`].

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

/// Canonical permission categories enforced by the gate (PRD §1.3 FR-06).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    FileRead,
    FileWrite,
    Network,
    Process,
    SystemInfo,
}

impl Permission {
    pub fn as_str(self) -> &'static str {
        match self {
            Permission::FileRead => "file_read",
            Permission::FileWrite => "file_write",
            Permission::Network => "network",
            Permission::Process => "process",
            Permission::SystemInfo => "system_info",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "file_read" | "fileread" | "read" => Some(Permission::FileRead),
            "file_write" | "filewrite" | "write" => Some(Permission::FileWrite),
            "network" | "net" => Some(Permission::Network),
            "process" | "proc" | "exec" => Some(Permission::Process),
            "system_info" | "systeminfo" | "sysinfo" => Some(Permission::SystemInfo),
            _ => None,
        }
    }

    pub fn all() -> [Permission; 5] {
        [
            Permission::FileRead,
            Permission::FileWrite,
            Permission::Network,
            Permission::Process,
            Permission::SystemInfo,
        ]
    }
}

/// Stable identity of a skill (matches the install slug used by `skills::*`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SkillId(pub String);

impl SkillId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[cfg(test)]
    pub fn test() -> Self {
        Self("test-skill".to_string())
    }
}

/// Outcome of a [`PermissionGate::check`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Allow,
    Deny,
}

/// In-memory grant matrix. Cheap to clone (Arc-wrapped) — share one instance
/// across the agent runtime.
#[derive(Debug, Clone, Default)]
pub struct PermissionGate {
    grants: Arc<RwLock<HashMap<SkillId, BTreeSet<Permission>>>>,
}

impl PermissionGate {
    pub fn new() -> Self {
        Self::default()
    }

    /// Grant a single permission category to a skill. Idempotent.
    pub fn grant(&self, skill: &SkillId, permission: Permission) {
        let mut guard = self.grants.write();
        guard.entry(skill.clone()).or_default().insert(permission);
    }

    /// Grant multiple permission categories to a skill.
    pub fn grant_many(&self, skill: &SkillId, permissions: &[Permission]) {
        let mut guard = self.grants.write();
        let set = guard.entry(skill.clone()).or_default();
        for &p in permissions {
            set.insert(p);
        }
    }

    /// Revoke a permission category from a skill. No-op if absent.
    pub fn revoke(&self, skill: &SkillId, permission: Permission) {
        let mut guard = self.grants.write();
        if let Some(set) = guard.get_mut(skill) {
            set.remove(&permission);
        }
    }

    /// Check whether a skill may exercise a permission category.
    ///
    /// Returns [`Decision::Deny`] by default; only explicit prior grants
    /// flip the decision to [`Decision::Allow`].
    pub fn check(&self, skill: &SkillId, permission: Permission) -> Decision {
        let guard = self.grants.read();
        match guard.get(skill) {
            Some(set) if set.contains(&permission) => Decision::Allow,
            _ => Decision::Deny,
        }
    }

    /// Return the granted set for a skill (deterministic order).
    pub fn granted(&self, skill: &SkillId) -> Vec<Permission> {
        let guard = self.grants.read();
        guard
            .get(skill)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_deny() {
        let gate = PermissionGate::new();
        let skill = SkillId::test();
        for p in Permission::all() {
            assert_eq!(gate.check(&skill, p), Decision::Deny, "permission {p:?}");
        }
    }

    #[test]
    fn allows_after_explicit_grant() {
        let gate = PermissionGate::new();
        let skill = SkillId::test();
        gate.grant(&skill, Permission::FileWrite);
        assert_eq!(gate.check(&skill, Permission::FileWrite), Decision::Allow);
        // Other permissions remain denied.
        assert_eq!(gate.check(&skill, Permission::FileRead), Decision::Deny);
    }

    #[test]
    fn revoke_restores_deny() {
        let gate = PermissionGate::new();
        let skill = SkillId::test();
        gate.grant(&skill, Permission::Network);
        assert_eq!(gate.check(&skill, Permission::Network), Decision::Allow);
        gate.revoke(&skill, Permission::Network);
        assert_eq!(gate.check(&skill, Permission::Network), Decision::Deny);
    }

    #[test]
    fn grant_many_records_all_categories() {
        let gate = PermissionGate::new();
        let skill = SkillId::test();
        gate.grant_many(
            &skill,
            &[Permission::FileRead, Permission::FileWrite, Permission::Network],
        );
        let granted = gate.granted(&skill);
        assert_eq!(granted.len(), 3);
        assert!(granted.contains(&Permission::FileRead));
        assert!(granted.contains(&Permission::FileWrite));
        assert!(granted.contains(&Permission::Network));
    }

    #[test]
    fn permission_parse_accepts_aliases() {
        assert_eq!(Permission::parse("file_read"), Some(Permission::FileRead));
        assert_eq!(Permission::parse("READ"), Some(Permission::FileRead));
        assert_eq!(Permission::parse("net"), Some(Permission::Network));
        assert_eq!(Permission::parse("sysinfo"), Some(Permission::SystemInfo));
        assert_eq!(Permission::parse("audio"), None);
    }

    #[test]
    fn grants_are_isolated_per_skill() {
        let gate = PermissionGate::new();
        let a = SkillId::new("skill-a");
        let b = SkillId::new("skill-b");
        gate.grant(&a, Permission::Process);
        assert_eq!(gate.check(&a, Permission::Process), Decision::Allow);
        assert_eq!(gate.check(&b, Permission::Process), Decision::Deny);
    }
}
