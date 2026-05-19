use serde_json::{Map, Value};

use crate::core::all::{ControllerFuture, RegisteredController};
use crate::core::{ControllerSchema, FieldSchema, TypeSchema};
use crate::rpc::RpcOutcome;

pub fn all_controller_schemas() -> Vec<ControllerSchema> {
    let mut out = vec![
        schemas("policy_info"),
        schemas("scan_input"),
        schemas("get_audit"),
        schemas("export_audit"),
    ];
    out.extend(crate::openhuman::security::cost_guard::ops::cost_guard_schemas());
    out.extend(crate::openhuman::security::permissions::ops::permissions_schemas());
    out
}

pub fn all_registered_controllers() -> Vec<RegisteredController> {
    let mut out = vec![
        RegisteredController {
            schema: schemas("policy_info"),
            handler: handle_policy_info,
        },
        RegisteredController {
            schema: schemas("scan_input"),
            handler: handle_scan_input,
        },
        RegisteredController {
            schema: schemas("get_audit"),
            handler: handle_get_audit,
        },
        RegisteredController {
            schema: schemas("export_audit"),
            handler: handle_export_audit,
        },
    ];
    out.extend(crate::openhuman::security::cost_guard::ops::cost_guard_registered_controllers());
    out.extend(crate::openhuman::security::permissions::ops::permissions_registered_controllers());
    out
}

pub fn schemas(function: &str) -> ControllerSchema {
    match function {
        "policy_info" => ControllerSchema {
            namespace: "security",
            function: "policy_info",
            description: "Return the active security/autonomy policy used by the core runtime.",
            inputs: vec![],
            outputs: vec![FieldSchema {
                name: "policy",
                ty: TypeSchema::Json,
                comment: "Security policy metadata and feature flags.",
                required: true,
            }],
        },
        "scan_input" => ControllerSchema {
            namespace: "security",
            function: "scan_input",
            description: "Scan user-provided text for prompt-injection and sensitive-data exfiltration patterns.",
            inputs: vec![FieldSchema {
                name: "text",
                ty: TypeSchema::String,
                comment: "User-provided text to scan before routing into agent or LLM context.",
                required: true,
            }],
            outputs: vec![
                FieldSchema {
                    name: "verdict",
                    ty: TypeSchema::Enum {
                        variants: vec!["allow", "review", "block"],
                    },
                    comment: "Guard verdict for the scanned input.",
                    required: true,
                },
                FieldSchema {
                    name: "score",
                    ty: TypeSchema::F64,
                    comment: "Risk score in the inclusive range 0.0 to 1.0.",
                    required: true,
                },
                FieldSchema {
                    name: "reasons",
                    ty: TypeSchema::Array(Box::new(TypeSchema::Json)),
                    comment: "Matched guard reasons with stable code and message fields.",
                    required: true,
                },
                FieldSchema {
                    name: "action",
                    ty: TypeSchema::Enum {
                        variants: vec!["allow", "block", "review_blocked"],
                    },
                    comment: "Enforcement action that the authoritative prompt guard would apply.",
                    required: true,
                },
                FieldSchema {
                    name: "prompt_hash",
                    ty: TypeSchema::String,
                    comment: "SHA-256 hash of the scanned text for correlation without logging raw content.",
                    required: true,
                },
                FieldSchema {
                    name: "prompt_chars",
                    ty: TypeSchema::U64,
                    comment: "Character count of the scanned text.",
                    required: true,
                },
            ],
        },
        "get_audit" => ControllerSchema {
            namespace: "security",
            function: "get_audit",
            description: "Read recent security audit JSONL records from the active OpenHuman data directory.",
            inputs: vec![
                FieldSchema {
                    name: "since",
                    ty: TypeSchema::String,
                    comment: "Optional RFC3339 lower-bound timestamp.",
                    required: false,
                },
                FieldSchema {
                    name: "limit",
                    ty: TypeSchema::U64,
                    comment: "Optional maximum number of records to return, clamped to 1..1000.",
                    required: false,
                },
            ],
            outputs: vec![
                FieldSchema {
                    name: "records",
                    ty: TypeSchema::Array(Box::new(TypeSchema::Json)),
                    comment: "Latest matching audit records in append order.",
                    required: true,
                },
                FieldSchema {
                    name: "has_more",
                    ty: TypeSchema::Bool,
                    comment: "True when more matching records exist before the returned window.",
                    required: true,
                },
                FieldSchema {
                    name: "source",
                    ty: TypeSchema::String,
                    comment: "Resolved audit log path used for the query.",
                    required: true,
                },
            ],
        },
        "export_audit" => ControllerSchema {
            namespace: "security",
            function: "export_audit",
            description: "Export filtered security audit records to a JSONL file.",
            inputs: vec![
                FieldSchema {
                    name: "path",
                    ty: TypeSchema::String,
                    comment: "Output JSONL file path.",
                    required: true,
                },
                FieldSchema {
                    name: "from",
                    ty: TypeSchema::String,
                    comment: "Optional RFC3339 inclusive lower-bound timestamp.",
                    required: false,
                },
                FieldSchema {
                    name: "to",
                    ty: TypeSchema::String,
                    comment: "Optional RFC3339 inclusive upper-bound timestamp.",
                    required: false,
                },
            ],
            outputs: vec![
                FieldSchema {
                    name: "exported",
                    ty: TypeSchema::U64,
                    comment: "Number of audit records written.",
                    required: true,
                },
                FieldSchema {
                    name: "file",
                    ty: TypeSchema::String,
                    comment: "Output JSONL file path.",
                    required: true,
                },
                FieldSchema {
                    name: "source",
                    ty: TypeSchema::String,
                    comment: "Resolved audit log path used as export source.",
                    required: true,
                },
            ],
        },
        _ => ControllerSchema {
            namespace: "security",
            function: "unknown",
            description: "Unknown security controller function.",
            inputs: vec![],
            outputs: vec![],
        },
    }
}

fn handle_policy_info(_params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async { to_json(crate::openhuman::security::rpc::security_policy_info()) })
}

fn handle_scan_input(params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move {
        let text = params
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| "security.scan_input requires string param `text`".to_string())?;
        to_json(crate::openhuman::security::rpc::security_scan_input(text))
    })
}

fn handle_get_audit(params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move {
        let since = optional_string(&params, "since")?;
        let limit = optional_u64(&params, "limit")?.map(|value| value as usize);
        to_json(crate::openhuman::security::rpc::security_get_audit(since.as_deref(), limit).await?)
    })
}

fn handle_export_audit(params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move {
        let path = params
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| "security.export_audit requires string param `path`".to_string())?;
        let from = optional_string(&params, "from")?;
        let to = optional_string(&params, "to")?;
        to_json(
            crate::openhuman::security::rpc::security_export_audit(
                path,
                from.as_deref(),
                to.as_deref(),
            )
            .await?,
        )
    })
}

fn to_json<T: serde::Serialize>(outcome: RpcOutcome<T>) -> Result<Value, String> {
    outcome.into_cli_compatible_json()
}

fn optional_string(params: &Map<String, Value>, key: &str) -> Result<Option<String>, String> {
    params
        .get(key)
        .map(|value| {
            value
                .as_str()
                .map(ToString::to_string)
                .ok_or_else(|| format!("security audit param `{key}` must be a string"))
        })
        .transpose()
}

fn optional_u64(params: &Map<String, Value>, key: &str) -> Result<Option<u64>, String> {
    params
        .get(key)
        .map(|value| {
            value
                .as_u64()
                .ok_or_else(|| format!("security audit param `{key}` must be a positive integer"))
        })
        .transpose()
}
