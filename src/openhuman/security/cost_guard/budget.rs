//! Budget definitions and scope boundary calculations.
//!
//! Scope semantics (PRD §3.2 / §2.4): boundaries are computed in UTC.
//! Weekly scope rolls over at Monday 00:00 UTC (ISO 8601 week).

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc, Weekday};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Budget aggregation period.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BudgetScope {
    Daily,
    Weekly,
    Monthly,
}

impl BudgetScope {
    pub fn as_str(self) -> &'static str {
        match self {
            BudgetScope::Daily => "daily",
            BudgetScope::Weekly => "weekly",
            BudgetScope::Monthly => "monthly",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "daily" | "day" => Some(BudgetScope::Daily),
            "weekly" | "week" => Some(BudgetScope::Weekly),
            "monthly" | "month" => Some(BudgetScope::Monthly),
            _ => None,
        }
    }

    /// Inclusive lower bound for the active window containing `now`.
    pub fn window_start(self, now: DateTime<Utc>) -> DateTime<Utc> {
        match self {
            BudgetScope::Daily => start_of_day_utc(now),
            BudgetScope::Weekly => start_of_iso_week_utc(now),
            BudgetScope::Monthly => start_of_month_utc(now),
        }
    }

    /// Exclusive upper bound for the active window containing `now`.
    pub fn window_end(self, now: DateTime<Utc>) -> DateTime<Utc> {
        match self {
            BudgetScope::Daily => start_of_day_utc(now) + Duration::days(1),
            BudgetScope::Weekly => start_of_iso_week_utc(now) + Duration::weeks(1),
            BudgetScope::Monthly => start_of_next_month_utc(now),
        }
    }
}

/// A persisted budget row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Budget {
    pub id: Uuid,
    pub scope: BudgetScope,
    pub limit_usd: f64,
    pub created_at: DateTime<Utc>,
    pub active: bool,
}

/// Outcome of a quota check on the hot path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaVerdict {
    Allow,
    Block,
}

fn start_of_day_utc(now: DateTime<Utc>) -> DateTime<Utc> {
    let d = now.date_naive();
    Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).expect("valid midnight"))
}

fn start_of_iso_week_utc(now: DateTime<Utc>) -> DateTime<Utc> {
    let day = now.date_naive();
    let from_monday = day.weekday().num_days_from_monday() as i64;
    let monday = day - Duration::days(from_monday);
    Utc.from_utc_datetime(&monday.and_hms_opt(0, 0, 0).expect("valid midnight"))
}

fn start_of_month_utc(now: DateTime<Utc>) -> DateTime<Utc> {
    let first = NaiveDate::from_ymd_opt(now.year(), now.month(), 1).expect("valid first of month");
    Utc.from_utc_datetime(&first.and_hms_opt(0, 0, 0).expect("valid midnight"))
}

fn start_of_next_month_utc(now: DateTime<Utc>) -> DateTime<Utc> {
    let (y, m) = if now.month() == 12 {
        (now.year() + 1, 1)
    } else {
        (now.year(), now.month() + 1)
    };
    let first = NaiveDate::from_ymd_opt(y, m, 1).expect("valid first of next month");
    Utc.from_utc_datetime(&first.and_hms_opt(0, 0, 0).expect("valid midnight"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(y: i32, m: u32, d: u32, h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, 0, 0).single().expect("valid timestamp")
    }

    #[test]
    fn scope_parse_accepts_aliases() {
        assert_eq!(BudgetScope::parse("daily"), Some(BudgetScope::Daily));
        assert_eq!(BudgetScope::parse("day"), Some(BudgetScope::Daily));
        assert_eq!(BudgetScope::parse("WEEK"), Some(BudgetScope::Weekly));
        assert_eq!(BudgetScope::parse("Monthly"), Some(BudgetScope::Monthly));
        assert_eq!(BudgetScope::parse("yearly"), None);
    }

    #[test]
    fn daily_window_is_midnight_to_next_midnight() {
        let now = ts(2026, 5, 19, 17);
        assert_eq!(BudgetScope::Daily.window_start(now), ts(2026, 5, 19, 0));
        assert_eq!(BudgetScope::Daily.window_end(now), ts(2026, 5, 20, 0));
    }

    #[test]
    fn weekly_window_rolls_over_on_monday_utc() {
        // 2026-05-19 is a Tuesday. Monday is 2026-05-18.
        let tue = ts(2026, 5, 19, 12);
        assert_eq!(BudgetScope::Weekly.window_start(tue), ts(2026, 5, 18, 0));
        assert_eq!(BudgetScope::Weekly.window_end(tue), ts(2026, 5, 25, 0));

        // On Sunday the active window is still last Monday..next Monday.
        let sun = ts(2026, 5, 24, 23);
        assert_eq!(BudgetScope::Weekly.window_start(sun), ts(2026, 5, 18, 0));
        assert_eq!(BudgetScope::Weekly.window_end(sun), ts(2026, 5, 25, 0));

        // Crossing into Monday opens a new window.
        let mon = ts(2026, 5, 25, 0);
        assert_eq!(BudgetScope::Weekly.window_start(mon), ts(2026, 5, 25, 0));
    }

    #[test]
    fn monthly_window_covers_full_month() {
        let now = ts(2026, 5, 19, 0);
        assert_eq!(BudgetScope::Monthly.window_start(now), ts(2026, 5, 1, 0));
        assert_eq!(BudgetScope::Monthly.window_end(now), ts(2026, 6, 1, 0));

        // December rolls into next year.
        let dec = ts(2026, 12, 15, 0);
        assert_eq!(BudgetScope::Monthly.window_end(dec), ts(2027, 1, 1, 0));
    }

    #[test]
    fn weekday_check_monday_is_zero_offset() {
        let mon = ts(2026, 5, 18, 12);
        assert_eq!(mon.weekday(), Weekday::Mon);
        assert_eq!(BudgetScope::Weekly.window_start(mon), ts(2026, 5, 18, 0));
    }
}
