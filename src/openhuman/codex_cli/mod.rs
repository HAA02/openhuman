//! Codex CLI lightweight LLM provider.
//!
//! Wraps the external `codex` binary in headless `exec` mode so the
//! desktop app can act as an LLM client in closed-network environments
//! without an HTTP API key. The pattern is documented in
//! `docs/CODEX_CLI_INTEGRATION.md`.

pub mod ops;
pub mod runner;
mod schemas;

pub use schemas::{
    all_controller_schemas as all_codex_cli_controller_schemas,
    all_registered_controllers as all_codex_cli_registered_controllers,
};
