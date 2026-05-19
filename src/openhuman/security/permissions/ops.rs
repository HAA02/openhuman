//! JSON-RPC controller surface for the permissions module.

use std::path::PathBuf;
use std::sync::OnceLock;

use serde_json::{json, Map, Value};

use crate::core::all::{ControllerFuture, RegisteredController};
use crate::core::{ControllerSchema, FieldSchema, TypeSchema};
use crate::rpc::RpcOutcome;

use super::gate::{Permission, PermissionGate, SkillId};
use super::manifest::{verify_manifest, ManifestVerdict};

/// Process-wide gate. Phase 5 wires UI grant flows through this singleton.
fn shared_gate() -> &'static PermissionGate {
    static GATE: OnceLock<PermissionGate> = OnceLock::new();
    GATE.get_or_init(PermissionGate::new)
}

pub fn permissions_schemas() -> Vec<ControllerSchema> {
    vec![schemas("verify_skill"), schemas("list_permissions")]
}

pub fn permissions_registered_controllers() -> Vec<RegisteredController> {
    vec![
        RegisteredController {
            schema: schemas("verify_skill"),
            handler: handle_verify_skill,
        },
        RegisteredController {
            schema: schemas("list_permissions"),
            handler: handle_list_permissions,
        },
    ]
}

fn schemas(function: &str) -> ControllerSchema {
    match function {
        "verify_skill" => ControllerSchema {
            namespace: "security",
            function: "verify_skill",
            description: "Verify a skill manifest file: parse, validate, compute SHA-256, optionally match an expected hash.",
            inputs: vec![
                FieldSchema {
                    name: "manifest_path",
                    ty: TypeSchema::String,
                    comment: "Filesystem path to the SKILL.md (or equivalent manifest).",
                    required: true,
                },
                FieldSchema {
                    name: "expected_sha256",
                    ty: TypeSchema::String,
                    comment: "Optional pinned SHA-256 (lowercase hex) to assert tampering protection.",
                    required: false,
                },
            ],
            outputs: vec![
                FieldSchema {
                    name: "verdict",
                    ty: TypeSchema::Enum {
                        variants: vec!["valid", "invalid"],
                    },
                    comment: "Overall pass/fail.",
                    required: true,
                },
                FieldSchema {
                    name: "sha256",
                    ty: TypeSchema::String,
                    comment: "SHA-256 of the manifest bytes (lowercase hex); empty on unreadable file.",
                    required: true,
                },
                FieldSchema {
                    name: "issues",
                    ty: TypeSchema::Array(Box::new(TypeSchema::String)),
                    comment: "Empty on Valid; otherwise one human-readable issue per element.",
                    required: true,
                },
                FieldSchema {
                    name: "path",
                    ty: TypeSchema::String,
                    comment: "Path echoed back for correlation.",
                    required: true,
                },
            ],
        },
        "list_permissions" => ControllerSchema {
            namespace: "security",
            function: "list_permissions",
            description: "Return the set of currently granted permissions for a skill_id (deny-all by default).",
            inputs: vec![FieldSchema {
                name: "skill_id",
                ty: TypeSchema::String,
                comment: "Stable skill identifier (matches the install slug).",
                required: true,
            }],
            outputs: vec![
                FieldSchema {
                    name: "skill_id",
                    ty: TypeSchema::String,
                    comment: "Echoed back.",
                    required: true,
                },
                FieldSchema {
                    name: "granted",
                    ty: TypeSchema::Array(Box::new(TypeSchema::String)),
                    comment: "Permission category strings (file_read/file_write/network/process/system_info).",
                    required: true,
                },
            ],
        },
        _ => ControllerSchema {
            namespace: "security",
            function: "unknown",
            description: "Unknown permissions controller function.",
            inputs: vec![],
            outputs: vec![],
        },
    }
}

fn handle_verify_skill(params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move {
        let manifest_path = params
            .get("manifest_path")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                "security.verify_skill requires string param `manifest_path`".to_string()
            })?;
        let expected = params
            .get("expected_sha256")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let report = verify_manifest(&PathBuf::from(manifest_path), expected.as_deref());
        let verdict_str = match report.verdict {
            ManifestVerdict::Valid => "valid",
            ManifestVerdict::Invalid => "invalid",
        };
        let payload = json!({
            "verdict": verdict_str,
            "sha256": report.sha256,
            "issues": report.issues,
            "path": report.path.display().to_string(),
        });
        let log = format!(
            "security_verify_skill verdict={} path={} issues={}",
            verdict_str,
            report.path.display(),
            report.issues.len()
        );
        if matches!(report.verdict, ManifestVerdict::Invalid) {
            tracing::warn!(verdict = "invalid", path = %report.path.display(), "{}", log);
        }
        to_json(RpcOutcome::single_log(payload, log))
    })
}

fn handle_list_permissions(params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move {
        let skill_id = params
            .get("skill_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "security.list_permissions requires non-empty `skill_id`".to_string())?
            .to_string();
        let skill = SkillId::new(skill_id.clone());
        let granted: Vec<_> = shared_gate()
            .granted(&skill)
            .into_iter()
            .map(Permission::as_str)
            .collect();
        let payload = json!({
            "skill_id": skill_id,
            "granted": granted,
        });
        to_json(RpcOutcome::single_log(
            payload,
            format!(
                "security_list_permissions skill_id={} granted={}",
                skill_id,
                granted.len()
            ),
        ))
    })
}

fn to_json<T: serde::Serialize>(outcome: RpcOutcome<T>) -> Result<Value, String> {
    outcome.into_cli_compatible_json()
}
