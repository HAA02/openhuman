//! Persistent ledger and budget storage (SQLite via rusqlite bundled).
//!
//! Schema (PRD §2.4):
//!   * `budgets` — active limit per scope (at most one active row per scope).
//!   * `cost_ledger` — append-only cost record correlated to audit records.
//!
//! Migrations are idempotent: callers may invoke `CostGuard::open` repeatedly.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::budget::{Budget, BudgetScope, QuotaVerdict};

/// Warning threshold (% of limit) at which `check_quota` emits a warning log.
const WARN_PERCENT: u8 = 80;

/// `cost_guard` storage handle. Safe to clone (shared connection behind a mutex).
#[derive(Clone)]
pub struct CostGuard {
    inner: Arc<Mutex<Connection>>,
    path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UsageReport {
    pub scope: BudgetScope,
    pub used_usd: f64,
    pub limit_usd: f64,
    pub percent: f32,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
}

impl CostGuard {
    /// Open a ledger at `<openhuman_dir>/cost/budget.db`, applying migrations.
    pub fn open(openhuman_dir: &Path) -> Result<Self> {
        let dir = openhuman_dir.join("cost");
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create cost dir at {}", dir.display()))?;
        let path = dir.join("budget.db");
        let conn = Connection::open(&path)
            .with_context(|| format!("failed to open cost ledger at {}", path.display()))?;
        Self::apply_migrations(&conn)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(conn)),
            path,
        })
    }

    /// Open an in-memory ledger for tests.
    pub fn new_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().context("failed to open in-memory ledger")?;
        Self::apply_migrations(&conn)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(conn)),
            path: PathBuf::from(":memory:"),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn apply_migrations(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS budgets (
                id TEXT PRIMARY KEY,
                scope TEXT NOT NULL CHECK(scope IN ('daily','weekly','monthly')),
                limit_usd REAL NOT NULL,
                active INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS cost_ledger (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts TEXT NOT NULL,
                scope TEXT NOT NULL,
                cost_usd REAL NOT NULL,
                model TEXT,
                session_id TEXT,
                audit_record_id TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_ledger_ts ON cost_ledger(ts);
            CREATE INDEX IF NOT EXISTS idx_ledger_scope_ts ON cost_ledger(scope, ts);
            "#,
        )
        .context("failed to apply cost_guard migrations")?;
        Ok(())
    }

    /// Create or replace the active budget for the given scope.
    pub fn set_budget(&self, scope: BudgetScope, limit_usd: f64) -> Result<Budget> {
        if !limit_usd.is_finite() || limit_usd < 0.0 {
            return Err(anyhow!(
                "budget limit must be a finite, non-negative value (got {limit_usd})"
            ));
        }
        let id = Uuid::new_v4();
        let now = Utc::now();
        let conn = self.inner.lock();
        // Deactivate any prior budgets for the same scope, then insert the new one.
        conn.execute(
            "UPDATE budgets SET active = 0 WHERE scope = ?1 AND active = 1",
            params![scope.as_str()],
        )?;
        conn.execute(
            "INSERT INTO budgets (id, scope, limit_usd, active, created_at) VALUES (?1, ?2, ?3, 1, ?4)",
            params![id.to_string(), scope.as_str(), limit_usd, now.to_rfc3339()],
        )?;
        Ok(Budget {
            id,
            scope,
            limit_usd,
            created_at: now,
            active: true,
        })
    }

    /// Return the active budget for the scope, if any.
    pub fn active_budget(&self, scope: BudgetScope) -> Result<Option<Budget>> {
        let conn = self.inner.lock();
        let row = conn
            .query_row(
                "SELECT id, scope, limit_usd, created_at FROM budgets WHERE scope = ?1 AND active = 1 LIMIT 1",
                params![scope.as_str()],
                |row| {
                    let id: String = row.get(0)?;
                    let scope_s: String = row.get(1)?;
                    let limit_usd: f64 = row.get(2)?;
                    let created_at: String = row.get(3)?;
                    Ok((id, scope_s, limit_usd, created_at))
                },
            )
            .optional()?;
        Ok(row.and_then(|(id, scope_s, limit_usd, created_at)| {
            let id = Uuid::parse_str(&id).ok()?;
            let scope = BudgetScope::parse(&scope_s)?;
            let created_at = DateTime::parse_from_rfc3339(&created_at)
                .ok()?
                .with_timezone(&Utc);
            Some(Budget {
                id,
                scope,
                limit_usd,
                created_at,
                active: true,
            })
        }))
    }

    /// Record a realized cost. `audit_record_id` correlates this row to an
    /// `AuditRecord` in the JSONL log (Phase 2).
    pub fn record_cost(
        &self,
        cost_usd: f64,
        model: Option<&str>,
        session_id: Option<&str>,
        audit_record_id: Option<&str>,
    ) -> Result<()> {
        if !cost_usd.is_finite() || cost_usd < 0.0 {
            return Err(anyhow!(
                "cost must be a finite, non-negative value (got {cost_usd})"
            ));
        }
        let now = Utc::now();
        let conn = self.inner.lock();
        // Records carry a 'scope' tag set to 'event' since they apply to all
        // scopes — aggregation slices the timestamp range per scope.
        conn.execute(
            "INSERT INTO cost_ledger (ts, scope, cost_usd, model, session_id, audit_record_id) VALUES (?1, 'event', ?2, ?3, ?4, ?5)",
            params![now.to_rfc3339(), cost_usd, model, session_id, audit_record_id],
        )?;
        Ok(())
    }

    /// Sum of `cost_usd` for the active window of `scope` containing `now`.
    pub fn used_in_window(&self, scope: BudgetScope, now: DateTime<Utc>) -> Result<f64> {
        let start = scope.window_start(now).to_rfc3339();
        let end = scope.window_end(now).to_rfc3339();
        let conn = self.inner.lock();
        let total: f64 = conn
            .query_row(
                "SELECT COALESCE(SUM(cost_usd), 0.0) FROM cost_ledger WHERE ts >= ?1 AND ts < ?2",
                params![start, end],
                |row| row.get(0),
            )
            .unwrap_or(0.0);
        Ok(total)
    }

    /// Produce a `UsageReport` for the active window of `scope`.
    pub fn usage(&self, scope: BudgetScope) -> Result<UsageReport> {
        let now = Utc::now();
        let used = self.used_in_window(scope, now)?;
        let limit_usd = self
            .active_budget(scope)?
            .map(|b| b.limit_usd)
            .unwrap_or(0.0);
        let percent = if limit_usd > 0.0 {
            ((used / limit_usd) * 100.0).clamp(0.0, f64::MAX) as f32
        } else {
            0.0
        };
        Ok(UsageReport {
            scope,
            used_usd: used,
            limit_usd,
            percent,
            window_start: scope.window_start(now),
            window_end: scope.window_end(now),
        })
    }

    /// Check whether a call with `estimated_usd` would exceed any active budget.
    ///
    /// Returns `(verdict, reason)`. Verdict is `Block` if **any** active scope
    /// would be exceeded; otherwise `Allow`. Warning logs are emitted at
    /// >= [`WARN_PERCENT`] but do not block.
    pub fn check_quota(&self, estimated_usd: f64) -> Result<(QuotaVerdict, Option<String>)> {
        if !estimated_usd.is_finite() || estimated_usd < 0.0 {
            return Err(anyhow!(
                "estimated cost must be a finite, non-negative value (got {estimated_usd})"
            ));
        }
        let now = Utc::now();
        let mut warning: Option<String> = None;
        for scope in [
            BudgetScope::Daily,
            BudgetScope::Weekly,
            BudgetScope::Monthly,
        ] {
            let Some(budget) = self.active_budget(scope)? else {
                continue;
            };
            let used = self.used_in_window(scope, now)?;
            let projected = used + estimated_usd;
            if projected > budget.limit_usd {
                let reason = format!(
                    "{} budget exceeded: ${:.4} + ${:.4} > ${:.4}",
                    scope.as_str(),
                    used,
                    estimated_usd,
                    budget.limit_usd
                );
                return Ok((QuotaVerdict::Block, Some(reason)));
            }
            if budget.limit_usd > 0.0 {
                let percent = (projected / budget.limit_usd) * 100.0;
                if percent >= WARN_PERCENT as f64 && warning.is_none() {
                    warning = Some(format!(
                        "{} budget at {:.0}% (${:.4}/${:.4})",
                        scope.as_str(),
                        percent,
                        projected,
                        budget.limit_usd
                    ));
                }
            }
        }
        Ok((QuotaVerdict::Allow, warning))
    }
}
