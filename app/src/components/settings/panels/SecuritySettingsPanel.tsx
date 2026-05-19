import { useState } from 'react';

import AuditView from '../../security/AuditView';
import BudgetForm from '../../security/BudgetForm';
import BudgetGauge, { BudgetScope } from '../../security/BudgetGauge';

const SCOPES: BudgetScope[] = ['daily', 'weekly', 'monthly'];

/**
 * Phase 5 settings panel that surfaces cost_guard + audit to the user.
 *
 * The page is intentionally self-contained — it does not yet plug into the
 * `useSettingsNavigation` breadcrumb stack so the union of routes does not
 * need to change in lockstep. Wiring through the navigation menu is a
 * follow-up that only edits `SettingsRoute` + `SettingsHome`.
 */
export default function SecuritySettingsPanel() {
  // A monotonically increasing counter that lets each BudgetForm trigger a
  // remount of its sibling BudgetGauge on successful save — cheap way to
  // force a refetch without lifting state into a store.
  const [gaugeNonce, setGaugeNonce] = useState(0);

  const handleSaved = () => {
    setGaugeNonce(value => value + 1);
  };

  return (
    <section
      data-testid="security-settings-panel"
      className="space-y-6 px-4 py-5 sm:px-6 lg:px-8"
    >
      <header className="space-y-1">
        <h1 className="text-xl font-semibold text-stone-900">보안 설정</h1>
        <p className="text-sm text-stone-600">
          예산 한도, 감사 로그, 권한 정책을 한 곳에서 관리합니다. 자세한 위협 모델은
          <a
            href="https://github.com/tinyhumansai/openhuman/blob/main/gitbooks/developing/security.md"
            target="_blank"
            rel="noreferrer noopener"
            className="ml-1 text-ocean-600 underline"
          >
            security.md
          </a>
          를 참고하세요.
        </p>
      </header>

      <section aria-labelledby="security-budget-heading" className="space-y-4">
        <h2 id="security-budget-heading" className="text-lg font-medium text-stone-900">
          예산 한도
        </h2>
        <div className="grid gap-4 md:grid-cols-3">
          {SCOPES.map(scope => (
            <div key={`${scope}-${gaugeNonce}`} className="space-y-3">
              <BudgetGauge scope={scope} refreshIntervalMs={30_000} />
              <BudgetForm scope={scope} onSaved={handleSaved} />
            </div>
          ))}
        </div>
      </section>

      <section aria-labelledby="security-audit-heading" className="space-y-3">
        <h2 id="security-audit-heading" className="text-lg font-medium text-stone-900">
          감사 로그
        </h2>
        <AuditView limit={50} />
      </section>
    </section>
  );
}
