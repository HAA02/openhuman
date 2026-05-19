import { FormEvent, useState } from 'react';

import { callCoreRpc } from '../../services/coreRpcClient';
import type { BudgetScope } from './BudgetGauge';

interface BudgetFormProps {
  scope: BudgetScope;
  /** Optional callback fired after a successful save. */
  onSaved?: (budget: { budget_id: string; limit_usd: number; scope: BudgetScope }) => void;
}

interface SetBudgetResponse {
  budget_id: string;
  scope: BudgetScope;
  limit_usd: number;
}

const SCOPE_LABEL: Record<BudgetScope, string> = { daily: '일일', weekly: '주간', monthly: '월간' };

/**
 * Budget input form bound to `security.set_budget` (PRD Phase 3 §2.3).
 *
 * Validates the entered limit on the client side (finite, non-negative) so
 * the Rust core's stricter validation only fires as a defense-in-depth check.
 */
export default function BudgetForm({ scope, onSaved }: BudgetFormProps) {
  const [limit, setLimit] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [savedMessage, setSavedMessage] = useState<string | null>(null);

  const parsedLimit = Number.parseFloat(limit);
  const isLimitValid = Number.isFinite(parsedLimit) && parsedLimit >= 0;

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!isLimitValid) {
      setError('한도는 0 이상의 숫자여야 합니다.');
      return;
    }
    setBusy(true);
    setError(null);
    setSavedMessage(null);
    try {
      const response = await callCoreRpc<SetBudgetResponse>({
        method: 'security.set_budget',
        params: { scope, limit_usd: parsedLimit },
      });
      setSavedMessage(
        `${SCOPE_LABEL[scope]} 한도가 $${response.limit_usd.toFixed(2)}로 저장되었습니다.`
      );
      onSaved?.(response);
    } catch (e) {
      setError(e instanceof Error ? e.message : '한도 저장에 실패했습니다.');
    } finally {
      setBusy(false);
    }
  }

  const inputId = `budget-limit-${scope}`;

  return (
    <form
      data-testid={`budget-form-${scope}`}
      onSubmit={handleSubmit}
      className="space-y-2"
      noValidate>
      <label htmlFor={inputId} className="block text-sm font-medium text-stone-700">
        {SCOPE_LABEL[scope]} 한도 (USD)
      </label>
      <div className="flex items-center gap-2">
        <span className="text-stone-500">$</span>
        <input
          id={inputId}
          type="number"
          inputMode="decimal"
          step="0.01"
          min="0"
          value={limit}
          onChange={event => setLimit(event.target.value)}
          disabled={busy}
          aria-invalid={limit.length > 0 && !isLimitValid}
          aria-describedby={error ? `${inputId}-error` : undefined}
          className="w-32 rounded-md border border-stone-300 px-2 py-1 text-sm focus:border-ocean-500 focus:outline-none focus:ring-1 focus:ring-ocean-500"
        />
        <button
          type="submit"
          disabled={busy || limit.length === 0}
          className="rounded-md bg-ocean-600 px-3 py-1 text-sm font-medium text-white hover:bg-ocean-700 disabled:cursor-not-allowed disabled:opacity-50">
          {busy ? '저장 중…' : '저장'}
        </button>
      </div>
      {error && (
        <p
          id={`${inputId}-error`}
          role="alert"
          data-testid={`budget-form-${scope}-error`}
          className="text-xs text-coral-600">
          {error}
        </p>
      )}
      {savedMessage && !error && (
        <p data-testid={`budget-form-${scope}-success`} className="text-xs text-sage-600">
          {savedMessage}
        </p>
      )}
    </form>
  );
}
