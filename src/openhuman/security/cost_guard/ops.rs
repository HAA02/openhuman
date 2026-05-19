//! JSON-RPC / CLI controller surface for cost_guard.

use std::path::PathBuf;

use serde_json::{json, Map, Value};

use crate::core::all::{ControllerFuture, RegisteredController};
use crate::core::{ControllerSchema, FieldSchema, TypeSchema};
use crate::openhuman::config::{default_root_openhuman_dir, load_config_with_timeout};
use crate::rpc::RpcOutcome;

use super::budget::{BudgetScope, QuotaVerdict};
use super::ledger::CostGuard;

/// Schemas registered under the `security` namespace.
pub fn cost_guard_schemas() -> Vec<ControllerSchema> {
    vec![
        schemas("set_budget"),
        schemas("get_usage"),
        schemas("check_quota"),
    ]
}

/// Controllers exposed for RPC dispatch.
pub fn cost_guard_registered_controllers() -> Vec<RegisteredController> {
    vec![
        RegisteredController {
            schema: schemas("set_budget"),
            handler: handle_set_budget,
        },
        RegisteredController {
            schema: schemas("get_usage"),
            handler: handle_get_usage,
        },
        RegisteredController {
            schema: schemas("check_quota"),
            handler: handle_check_quota,
        },
    ]
}

fn schemas(function: &str) -> ControllerSchema {
    match function {
        "set_budget" => ControllerSchema {
            namespace: "security",
            function: "set_budget",
            description: "Create or replace the active spending budget for a scope (daily/weekly/monthly).",
            inputs: vec![
                FieldSchema {
                    name: "scope",
                    ty: TypeSchema::Enum {
                        variants: vec!["daily", "weekly", "monthly"],
                    },
                    comment: "Budget aggregation window.",
                    required: true,
                },
                FieldSchema {
                    name: "limit_usd",
                    ty: TypeSchema::F64,
                    comment: "Limit in USD; finite, non-negative.",
                    required: true,
                },
            ],
            outputs: vec![
                FieldSchema {
                    name: "budget_id",
                    ty: TypeSchema::String,
                    comment: "UUID v4 of the newly created budget row.",
                    required: true,
                },
                FieldSchema {
                    name: "scope",
                    ty: TypeSchema::String,
                    comment: "Scope echoed back.",
                    required: true,
                },
                FieldSchema {
                    name: "limit_usd",
                    ty: TypeSchema::F64,
                    comment: "Limit echoed back.",
                    required: true,
                },
            ],
        },
        "get_usage" => ControllerSchema {
            namespace: "security",
            function: "get_usage",
            description: "Return cost used in the active window for a given scope.",
            inputs: vec![FieldSchema {
                name: "scope",
                ty: TypeSchema::Enum {
                    variants: vec!["daily", "weekly", "monthly"],
                },
                comment: "Scope to report on.",
                required: true,
            }],
            outputs: vec![
                FieldSchema {
                    name: "used_usd",
                    ty: TypeSchema::F64,
                    comment: "Sum of recorded costs in the active window.",
                    required: true,
                },
                FieldSchema {
                    name: "limit_usd",
                    ty: TypeSchema::F64,
                    comment: "Active budget limit, or 0.0 if no active budget.",
                    required: true,
                },
                FieldSchema {
                    name: "percent",
                    ty: TypeSchema::F64,
                    comment: "used / limit * 100, or 0.0 if no limit set.",
                    required: true,
                },
                FieldSchema {
                    name: "window_start",
                    ty: TypeSchema::String,
                    comment: "RFC3339 inclusive lower bound of the active window.",
                    required: true,
                },
                FieldSchema {
                    name: "window_end",
                    ty: TypeSchema::String,
                    comment: "RFC3339 exclusive upper bound of the active window.",
                    required: true,
                },
            ],
        },
        "check_quota" => ControllerSchema {
            namespace: "security",
            function: "check_quota",
            description: "Decide whether a call with `estimated_usd` would exceed any active budget. Fail-safe: returns block on quota breach.",
            inputs: vec![FieldSchema {
                name: "estimated_usd",
                ty: TypeSchema::F64,
                comment: "Estimated cost of the upcoming call. Must be finite and non-negative.",
                required: true,
            }],
            outputs: vec![
                FieldSchema {
                    name: "verdict",
                    ty: TypeSchema::Enum {
                        variants: vec!["allow", "block"],
                    },
                    comment: "Quota decision.",
                    required: true,
                },
                FieldSchema {
                    name: "reason",
                    ty: TypeSchema::String,
                    comment: "Human-readable explanation; empty string on allow without warning.",
                    required: true,
                },
            ],
        },
        _ => ControllerSchema {
            namespace: "security",
            function: "unknown",
            description: "Unknown cost_guard controller function.",
            inputs: vec![],
            outputs: vec![],
        },
    }
}

fn handle_set_budget(params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move {
        let scope = parse_scope(&params)?;
        let limit_usd = params
            .get("limit_usd")
            .and_then(Value::as_f64)
            .ok_or_else(|| "security.set_budget requires numeric `limit_usd`".to_string())?;
        let guard = open_guard().await?;
        let budget = guard
            .set_budget(scope, limit_usd)
            .map_err(|e| format!("failed to persist budget: {e}"))?;
        let payload = json!({
            "budget_id": budget.id.to_string(),
            "scope": budget.scope.as_str(),
            "limit_usd": budget.limit_usd,
        });
        to_json(RpcOutcome::single_log(
            payload,
            format!(
                "security_set_budget scope={} limit_usd={:.4}",
                scope.as_str(),
                limit_usd
            ),
        ))
    })
}

fn handle_get_usage(params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move {
        let scope = parse_scope(&params)?;
        let guard = open_guard().await?;
        let report = guard
            .usage(scope)
            .map_err(|e| format!("failed to read usage: {e}"))?;
        let payload = json!({
            "scope": report.scope.as_str(),
            "used_usd": report.used_usd,
            "limit_usd": report.limit_usd,
            "percent": report.percent,
            "window_start": report.window_start.to_rfc3339(),
            "window_end": report.window_end.to_rfc3339(),
        });
        to_json(RpcOutcome::single_log(
            payload,
            format!(
                "security_get_usage scope={} used_usd={:.4} percent={:.1}",
                scope.as_str(),
                report.used_usd,
                report.percent
            ),
        ))
    })
}

fn handle_check_quota(params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move {
        let estimated_usd = params
            .get("estimated_usd")
            .and_then(Value::as_f64)
            .ok_or_else(|| "security.check_quota requires numeric `estimated_usd`".to_string())?;
        let guard = open_guard().await?;
        let (verdict, reason) = guard
            .check_quota(estimated_usd)
            .map_err(|e| format!("failed to check quota: {e}"))?;
        let verdict_str = match verdict {
            QuotaVerdict::Allow => "allow",
            QuotaVerdict::Block => "block",
        };
        let reason_text = reason.clone().unwrap_or_default();
        let payload = json!({
            "verdict": verdict_str,
            "reason": reason_text,
        });
        let log_line = match (&verdict, &reason) {
            (QuotaVerdict::Block, Some(r)) => {
                format!("security_check_quota blocked: {r}")
            }
            (QuotaVerdict::Allow, Some(r)) => {
                format!("security_check_quota allowed with warning: {r}")
            }
            (QuotaVerdict::Allow, None) => "security_check_quota allowed".to_string(),
            (QuotaVerdict::Block, None) => "security_check_quota blocked".to_string(),
        };
        if matches!(verdict, QuotaVerdict::Block) {
            tracing::warn!(verdict = "block", estimated_usd, "{}", log_line);
        }
        to_json(RpcOutcome::single_log(payload, log_line))
    })
}

fn parse_scope(params: &Map<String, Value>) -> Result<BudgetScope, String> {
    let raw = params
        .get("scope")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing string param `scope`".to_string())?;
    BudgetScope::parse(raw)
        .ok_or_else(|| format!("unknown scope `{raw}`; expected daily/weekly/monthly"))
}

async fn open_guard() -> Result<CostGuard, String> {
    let dir = active_data_dir().await;
    CostGuard::open(&dir)
        .map_err(|e| format!("failed to open cost guard at {}: {e}", dir.display()))
}

async fn active_data_dir() -> PathBuf {
    match load_config_with_timeout().await {
        Ok(config) => config
            .config_path
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                default_root_openhuman_dir().unwrap_or_else(|_| PathBuf::from(".openhuman"))
            }),
        Err(err) => {
            tracing::warn!(
                error = %err,
                "[cost_guard] using fallback data dir after config load failure"
            );
            default_root_openhuman_dir().unwrap_or_else(|_| PathBuf::from(".openhuman"))
        }
    }
}

fn to_json<T: serde::Serialize>(outcome: RpcOutcome<T>) -> Result<Value, String> {
    outcome.into_cli_compatible_json()
}
