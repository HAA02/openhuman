import { useCallback, useEffect, useState } from 'react';

import { callCoreRpc } from '../../services/coreRpcClient';

export type BudgetScope = 'daily' | 'weekly' | 'monthly';

interface UsagePayload {
  scope: BudgetScope;
  used_usd: number;
  limit_usd: number;
  percent: number;
  window_start: string;
  window_end: string;
}

interface BudgetGaugeProps {
  scope: BudgetScope;
  /** Refresh interval in ms. `null` disables auto-refresh. Default 30s. */
  refreshIntervalMs?: number | null;
}

const SCOPE_LABEL: Record<BudgetScope, string> = {
  daily: '오늘',
  weekly: '이번 주',
  monthly: '이번 달',
};

function formatUsd(value: number): string {
  if (!Number.isFinite(value)) return '$0.00';
  return `$${value.toFixed(2)}`;
}

function gaugeColorClass(percent: number): string {
  if (percent >= 80) return 'bg-coral-500';
  if (percent >= 60) return 'bg-amber-500';
  return 'bg-sage-500';
}

function statusLabel(usage: UsagePayload): string {
  if (usage.limit_usd <= 0) return '한도 미설정';
  if (usage.percent >= 100) return '한도 초과';
  if (usage.percent >= 80) return '경고';
  return '정상';
}

/**
 * Renders a daily/weekly/monthly cost-budget gauge backed by
 * `security.get_usage` from the Rust core (PRD Phase 3 §2.3).
 *
 * The component is intentionally read-only — budget mutation lives in
 * `SecuritySettingsPanel`. Color thresholds match the PRD warning rule
 * (≥ 80% = warning, ≥ 100% = breach).
 */
export default function BudgetGauge({ scope, refreshIntervalMs = 30_000 }: BudgetGaugeProps) {
  const [usage, setUsage] = useState<UsagePayload | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    try {
      const data = await callCoreRpc<UsagePayload>({
        method: 'security.get_usage',
        params: { scope },
      });
      setUsage(data);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'failed to fetch usage');
    } finally {
      setLoading(false);
    }
  }, [scope]);

  useEffect(() => {
    void refresh();
    if (refreshIntervalMs == null) return;
    const id = window.setInterval(() => {
      void refresh();
    }, refreshIntervalMs);
    return () => {
      window.clearInterval(id);
    };
  }, [refresh, refreshIntervalMs]);

  if (loading && !usage) {
    return (
      <div data-testid="budget-gauge-loading" className="rounded-md border border-stone-200 p-4">
        <p className="text-sm text-stone-500">예산 사용량을 불러오는 중…</p>
      </div>
    );
  }

  if (error) {
    return (
      <div
        data-testid="budget-gauge-error"
        role="alert"
        className="rounded-md border border-coral-200 bg-coral-50 p-4 text-sm text-coral-700">
        예산 정보를 불러오지 못했습니다: {error}
      </div>
    );
  }

  if (!usage) return null;

  const clampedPercent = Math.min(Math.max(usage.percent, 0), 100);
  const status = statusLabel(usage);

  return (
    <section
      data-testid={`budget-gauge-${scope}`}
      data-scope={scope}
      aria-label={`${SCOPE_LABEL[scope]} 예산 게이지`}
      className="rounded-md border border-stone-200 bg-white p-4">
      <header className="mb-2 flex items-baseline justify-between">
        <h3 className="text-sm font-medium text-stone-700">{SCOPE_LABEL[scope]} 예산</h3>
        <span data-testid={`budget-gauge-${scope}-status`} className="text-xs text-stone-500">
          {status}
        </span>
      </header>

      <div className="mb-2 flex items-baseline gap-2">
        <span className="text-2xl font-semibold text-stone-900">{formatUsd(usage.used_usd)}</span>
        <span className="text-sm text-stone-500">/ {formatUsd(usage.limit_usd)}</span>
      </div>

      <div
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(clampedPercent)}
        aria-label={`${SCOPE_LABEL[scope]} 사용률 ${clampedPercent.toFixed(0)}%`}
        className="h-2 w-full overflow-hidden rounded-full bg-stone-100">
        <div
          data-testid={`budget-gauge-${scope}-fill`}
          className={`h-full transition-[width] duration-300 ${gaugeColorClass(usage.percent)}`}
          style={{ width: `${clampedPercent}%` }}
        />
      </div>

      <p className="mt-2 text-xs text-stone-500">{clampedPercent.toFixed(1)}% 사용</p>
    </section>
  );
}
