use serde_json::{Map, Value};

use crate::core::all::{ControllerFuture, RegisteredController};
use crate::core::{ControllerSchema, FieldSchema, TypeSchema};
use crate::rpc::RpcOutcome;

pub fn all_controller_schemas() -> Vec<ControllerSchema> {
    vec![schemas("policy_info"), schemas("scan_input")]
}

pub fn all_registered_controllers() -> Vec<RegisteredController> {
    vec![
        RegisteredController {
            schema: schemas("policy_info"),
            handler: handle_policy_info,
        },
        RegisteredController {
            schema: schemas("scan_input"),
            handler: handle_scan_input,
        },
    ]
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

fn to_json<T: serde::Serialize>(outcome: RpcOutcome<T>) -> Result<Value, String> {
    outcome.into_cli_compatible_json()
}
