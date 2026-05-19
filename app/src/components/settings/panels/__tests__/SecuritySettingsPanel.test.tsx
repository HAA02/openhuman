import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import SecuritySettingsPanel from '../SecuritySettingsPanel';

const { mockCallCoreRpc } = vi.hoisted(() => ({ mockCallCoreRpc: vi.fn() }));

vi.mock('../../../../services/coreRpcClient', () => ({ callCoreRpc: mockCallCoreRpc }));

function usageResponse(scope: 'daily' | 'weekly' | 'monthly', used_usd: number, limit_usd: number) {
  return {
    scope,
    used_usd,
    limit_usd,
    percent: limit_usd > 0 ? (used_usd / limit_usd) * 100 : 0,
    window_start: '2026-05-19T00:00:00+00:00',
    window_end: '2026-05-20T00:00:00+00:00',
  };
}

function configureMocks() {
  mockCallCoreRpc.mockImplementation(async ({ method, params }) => {
    if (method === 'security.get_usage') {
      const scope = (params as { scope: 'daily' | 'weekly' | 'monthly' }).scope;
      return usageResponse(scope, 0.2, 1.0);
    }
    if (method === 'security.get_audit') {
      return { records: [], has_more: false, source: '/tmp/audit.log' };
    }
    throw new Error(`unexpected method ${method}`);
  });
}

describe('SecuritySettingsPanel', () => {
  beforeEach(() => {
    mockCallCoreRpc.mockReset();
    configureMocks();
  });

  it('renders three budget gauges (daily / weekly / monthly)', async () => {
    render(<SecuritySettingsPanel />);
    await waitFor(() => {
      expect(screen.getByTestId('budget-gauge-daily')).toBeTruthy();
    });
    expect(screen.getByTestId('budget-gauge-weekly')).toBeTruthy();
    expect(screen.getByTestId('budget-gauge-monthly')).toBeTruthy();
  });

  it('renders three budget forms (daily / weekly / monthly)', () => {
    render(<SecuritySettingsPanel />);
    expect(screen.getByTestId('budget-form-daily')).toBeTruthy();
    expect(screen.getByTestId('budget-form-weekly')).toBeTruthy();
    expect(screen.getByTestId('budget-form-monthly')).toBeTruthy();
  });

  it('renders the audit view region', async () => {
    render(<SecuritySettingsPanel />);
    await waitFor(() => {
      expect(screen.getByTestId('audit-view')).toBeTruthy();
    });
  });

  it('calls security.get_usage for every scope on mount', async () => {
    render(<SecuritySettingsPanel />);
    await waitFor(() => {
      const calls = mockCallCoreRpc.mock.calls.map(call => call[0]?.method);
      expect(calls).toContain('security.get_usage');
      expect(calls).toContain('security.get_audit');
    });
    const scopesRequested = mockCallCoreRpc.mock.calls
      .filter(call => call[0]?.method === 'security.get_usage')
      .map(call => (call[0]?.params as { scope: string }).scope);
    expect(scopesRequested).toEqual(expect.arrayContaining(['daily', 'weekly', 'monthly']));
  });

  it('links to the security threat model gitbook', () => {
    render(<SecuritySettingsPanel />);
    const link = screen.getByRole('link', { name: 'security.md' });
    expect(link.getAttribute('href')).toContain('/gitbooks/developing/security.md');
    expect(link.getAttribute('rel')).toContain('noopener');
  });
});
