//! RPC handlers for the `codex` namespace.

use serde_json::{json, Map, Value};

use crate::core::all::ControllerFuture;
use crate::rpc::RpcOutcome;

use super::runner::{run_headless, CodexOptions};

const MAX_TIMEOUT_MS: u64 = 600_000;

pub fn handle_status(_params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move {
        let (available, version, error) = match super::runner::probe() {
            Ok(v) => (true, v, String::new()),
            Err(e) => (false, String::new(), e),
        };
        let payload = json!({
            "available": available,
            "version": version,
            "error": error,
        });
        let log = if available {
            format!("codex_status available version={version}")
        } else {
            format!("codex_status unavailable error={error}")
        };
        to_json(RpcOutcome::single_log(payload, log))
    })
}

pub fn handle_complete(params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move {
        let prompt = params
            .get("prompt")
            .and_then(Value::as_str)
            .ok_or_else(|| "codex.complete requires string param `prompt`".to_string())?
            .to_string();
        let model = params
            .get("model")
            .and_then(Value::as_str)
            .map(|s| s.to_string());
        let timeout_ms = params
            .get("timeout_ms")
            .and_then(Value::as_u64)
            .map(|v| v.min(MAX_TIMEOUT_MS));

        let opts = CodexOptions {
            model,
            cwd: None,
            timeout_ms,
            sandbox: None,
        };

        let result = tokio::task::spawn_blocking(move || run_headless(&prompt, opts))
            .await
            .map_err(|e| format!("codex worker join failed: {e}"))??;

        let payload = json!({
            "content": result.content,
            "model": result.model,
            "elapsed_ms": result.elapsed_ms,
        });
        let log = format!(
            "codex_complete ok model={} elapsed_ms={} chars={}",
            result.model,
            result.elapsed_ms,
            result.content.chars().count()
        );
        to_json(RpcOutcome::single_log(payload, log))
    })
}

fn to_json<T: serde::Serialize>(outcome: RpcOutcome<T>) -> Result<Value, String> {
    outcome.into_cli_compatible_json()
}
