//! Controller schemas for the `codex` namespace.

use crate::core::all::{ControllerFuture, RegisteredController};
use crate::core::{ControllerSchema, FieldSchema, TypeSchema};

use super::ops;

pub fn all_controller_schemas() -> Vec<ControllerSchema> {
    vec![schemas("status"), schemas("complete")]
}

pub fn all_registered_controllers() -> Vec<RegisteredController> {
    vec![
        RegisteredController {
            schema: schemas("status"),
            handler: ops::handle_status,
        },
        RegisteredController {
            schema: schemas("complete"),
            handler: ops::handle_complete,
        },
    ]
}

pub fn schemas(function: &str) -> ControllerSchema {
    match function {
        "status" => ControllerSchema {
            namespace: "codex",
            function: "status",
            description: "Probe the local codex CLI binary; returns whether it is invocable and its reported version.",
            inputs: vec![],
            outputs: vec![
                FieldSchema {
                    name: "available",
                    ty: TypeSchema::Bool,
                    comment: "True when `codex --version` exits with status 0.",
                    required: true,
                },
                FieldSchema {
                    name: "version",
                    ty: TypeSchema::String,
                    comment: "Codex CLI version string; empty when unavailable.",
                    required: true,
                },
                FieldSchema {
                    name: "error",
                    ty: TypeSchema::String,
                    comment: "Error message when the probe failed; empty on success.",
                    required: true,
                },
            ],
        },
        "complete" => ControllerSchema {
            namespace: "codex",
            function: "complete",
            description: "Run a single-turn LLM completion via `codex exec` headlessly. Returns the assistant's last message.",
            inputs: vec![
                FieldSchema {
                    name: "prompt",
                    ty: TypeSchema::String,
                    comment: "Prompt sent to codex on stdin; must be non-empty.",
                    required: true,
                },
                FieldSchema {
                    name: "model",
                    ty: TypeSchema::String,
                    comment: "Override the model (defaults to CODEX_MODEL env or gpt-5.4-mini).",
                    required: false,
                },
                FieldSchema {
                    name: "timeout_ms",
                    ty: TypeSchema::U64,
                    comment: "Hard timeout in milliseconds; capped at 600000.",
                    required: false,
                },
            ],
            outputs: vec![
                FieldSchema {
                    name: "content",
                    ty: TypeSchema::String,
                    comment: "Last assistant message captured from codex.",
                    required: true,
                },
                FieldSchema {
                    name: "model",
                    ty: TypeSchema::String,
                    comment: "Model that produced the response.",
                    required: true,
                },
                FieldSchema {
                    name: "elapsed_ms",
                    ty: TypeSchema::U64,
                    comment: "Wall-clock time spent in the codex subprocess.",
                    required: true,
                },
            ],
        },
        _ => ControllerSchema {
            namespace: "codex",
            function: "unknown",
            description: "Unknown codex controller function.",
            inputs: vec![],
            outputs: vec![],
        },
    }
}

#[allow(dead_code)]
fn _enforce_signature(_: ControllerFuture) {}
