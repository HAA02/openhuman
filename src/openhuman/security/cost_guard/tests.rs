//! Unit tests for cost_guard (PRD §3.1 Phase 3 Red phase).

use super::budget::{BudgetScope, QuotaVerdict};
use super::ledger::CostGuard;
use super::ops::{cost_guard_registered_controllers, cost_guard_schemas};

#[test]
fn allows_call_within_budget() {
    let guard = CostGuard::new_in_memory().expect("open in-memory ledger");
    guard
        .set_budget(BudgetScope::Daily, 1.0)
        .expect("set daily budget");
    let (verdict, _reason) = guard.check_quota(0.20).expect("check quota");
    assert_eq!(verdict, QuotaVerdict::Allow);
}

#[test]
fn blocks_call_exceeding_daily_budget() {
    let guard = CostGuard::new_in_memory().expect("open in-memory ledger");
    guard
        .set_budget(BudgetScope::Daily, 1.0)
        .expect("set daily budget");
    guard
        .record_cost(0.95, Some("test-model"), None, None)
        .expect("record cost");
    let (verdict, reason) = guard.check_quota(0.10).expect("check quota");
    assert_eq!(verdict, QuotaVerdict::Block);
    assert!(reason.expect("reason present").contains("daily"));
}

#[test]
fn emits_warning_at_80_percent() {
    let guard = CostGuard::new_in_memory().expect("open in-memory ledger");
    guard
        .set_budget(BudgetScope::Daily, 1.0)
        .expect("set daily budget");
    guard
        .record_cost(0.75, Some("test-model"), None, None)
        .expect("record cost");
    // 0.75 + 0.10 = 0.85 → 85% of $1 → warning, but allow.
    let (verdict, reason) = guard.check_quota(0.10).expect("check quota");
    assert_eq!(verdict, QuotaVerdict::Allow);
    let reason = reason.expect("warning reason present at 80%+");
    assert!(reason.contains("daily"));
    assert!(reason.contains("%"));
}

#[test]
fn weekly_scope_aggregates_across_days() {
    // We can't easily mock the clock without injecting it, but we can verify
    // that the weekly window contains the current Daily window and is wider
    // than it. (Behavior under a clock change is exercised in
    // budget.rs::tests::weekly_window_rolls_over_on_monday_utc.)
    let guard = CostGuard::new_in_memory().expect("open in-memory ledger");
    guard
        .set_budget(BudgetScope::Weekly, 5.0)
        .expect("set weekly budget");
    guard
        .record_cost(2.0, Some("test-model"), None, None)
        .expect("first cost");
    guard
        .record_cost(2.5, Some("test-model"), None, None)
        .expect("second cost");
    let usage = guard.usage(BudgetScope::Weekly).expect("usage");
    assert!((usage.used_usd - 4.5).abs() < 1e-9);
    assert!((usage.limit_usd - 5.0).abs() < 1e-9);
    // 4.5 + 0.6 = 5.1 > 5.0 — must block on weekly.
    let (verdict, reason) = guard.check_quota(0.6).expect("check quota");
    assert_eq!(verdict, QuotaVerdict::Block);
    assert!(reason.expect("reason").contains("weekly"));
}

#[test]
fn replacing_budget_deactivates_prior_row() {
    let guard = CostGuard::new_in_memory().expect("open in-memory ledger");
    let first = guard
        .set_budget(BudgetScope::Monthly, 50.0)
        .expect("set initial monthly budget");
    let second = guard
        .set_budget(BudgetScope::Monthly, 100.0)
        .expect("replace monthly budget");
    assert_ne!(first.id, second.id);
    let active = guard
        .active_budget(BudgetScope::Monthly)
        .expect("query active budget")
        .expect("active budget present");
    assert_eq!(active.id, second.id);
    assert!((active.limit_usd - 100.0).abs() < 1e-9);
}

#[test]
fn rejects_non_finite_or_negative_limit() {
    let guard = CostGuard::new_in_memory().expect("open in-memory ledger");
    assert!(guard.set_budget(BudgetScope::Daily, -1.0).is_err());
    assert!(guard.set_budget(BudgetScope::Daily, f64::NAN).is_err());
    assert!(guard.set_budget(BudgetScope::Daily, f64::INFINITY).is_err());
}

#[test]
fn rejects_non_finite_or_negative_estimate() {
    let guard = CostGuard::new_in_memory().expect("open in-memory ledger");
    assert!(guard.check_quota(-0.01).is_err());
    assert!(guard.check_quota(f64::NAN).is_err());
}

#[test]
fn migration_is_idempotent() {
    // PRD V3-4: applying migrations twice yields the same schema state.
    use tempfile::TempDir;
    let tmp = TempDir::new().expect("tempdir");
    let guard1 = CostGuard::open(tmp.path()).expect("first open");
    guard1
        .set_budget(BudgetScope::Daily, 2.0)
        .expect("budget on first open");
    drop(guard1);
    // Re-open the same db; data and schema must survive.
    let guard2 = CostGuard::open(tmp.path()).expect("second open");
    let active = guard2
        .active_budget(BudgetScope::Daily)
        .expect("query")
        .expect("budget persisted across opens");
    assert!((active.limit_usd - 2.0).abs() < 1e-9);
}

#[test]
fn usage_is_zero_when_no_budget_set() {
    let guard = CostGuard::new_in_memory().expect("open in-memory ledger");
    let usage = guard.usage(BudgetScope::Daily).expect("usage report");
    assert_eq!(usage.used_usd, 0.0);
    assert_eq!(usage.limit_usd, 0.0);
    assert_eq!(usage.percent, 0.0);
}

#[test]
fn check_quota_with_no_active_budget_allows() {
    let guard = CostGuard::new_in_memory().expect("open in-memory ledger");
    let (verdict, reason) = guard.check_quota(1000.0).expect("check quota");
    assert_eq!(verdict, QuotaVerdict::Allow);
    assert!(reason.is_none());
}

#[test]
fn cost_guard_schemas_advertise_three_methods() {
    let names: Vec<_> = cost_guard_schemas()
        .into_iter()
        .map(|s| s.method_name())
        .collect();
    assert_eq!(
        names,
        vec![
            "security.set_budget".to_string(),
            "security.get_usage".to_string(),
            "security.check_quota".to_string(),
        ]
    );
}

#[test]
fn cost_guard_registered_controllers_match_schema_count() {
    let schemas = cost_guard_schemas();
    let controllers = cost_guard_registered_controllers();
    assert_eq!(controllers.len(), schemas.len());
}
