//! Cost guard — daily/weekly/monthly budget enforcement.
//!
//! Implements PRD §2.2 cost_guard: persistent SQLite-backed budgets and a
//! cost ledger correlated to audit records. Distinct from the JSONL-based
//! token tracker in [`crate::openhuman::cost`], which records per-session
//! token usage. cost_guard answers the "is this LLM call within budget?"
//! question on the hot path.

pub mod budget;
pub mod ledger;
pub mod ops;

#[cfg(test)]
mod tests;

pub use budget::{Budget, BudgetScope, QuotaVerdict};
pub use ledger::{CostGuard, UsageReport};
