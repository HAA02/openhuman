//! Skill permission gating (PRD §2.2 permissions).
//!
//! * `manifest` — parse a SKILL.md frontmatter and compute its SHA-256.
//! * `gate` — runtime grant/check matrix over the five canonical permission
//!   categories: file_read, file_write, network, process, system_info.
//! * `ops` — JSON-RPC controller surface for `security.verify_skill` and
//!   `security.list_permissions`.

pub mod gate;
pub mod manifest;
pub mod ops;

#[cfg(test)]
mod tests;

pub use gate::{Decision, Permission, PermissionGate, SkillId};
pub use manifest::{ManifestVerdict, ManifestVerification};
