import { render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import BudgetGauge from '../BudgetGauge';

const { mockCallCoreRpc } = vi.hoisted(() => ({
  mockCallCoreRpc: vi.fn(),
}));

vi.mock('../../../services/coreRpcClient', () => ({
  callCoreRpc: mockCallCoreRpc,
}));

const sample = (overrides: Partial<{
  used_usd: number;
  limit_usd: number;
  percent: number;
}> = {}) => ({
  scope: 'daily' as const,
  used_usd: overrides.used_usd ?? 0.4,
  limit_usd: overrides.limit_usd ?? 1.0,
  percent: overrides.percent ?? 40,
  window_start: '2026-05-19T00:00:00+00:00',
  window_end: '2026-05-20T00:00:00+00:00',
});

describe('BudgetGauge', () => {
  beforeEach(() => {
    mockCallCoreRpc.mockReset();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('renders the loading state before the first response', () => {
    mockCallCoreRpc.mockReturnValue(new Promise(() => {}));
    render(<BudgetGauge scope="daily" refreshIntervalMs={null} />);
    expect(screen.getByTestId('budget-gauge-loading')).toBeTruthy();
  });

  it('calls security.get_usage with the requested scope', async () => {
    mockCallCoreRpc.mockResolvedValue(sample());
    render(<BudgetGauge scope="weekly" refreshIntervalMs={null} />);
    await waitFor(() => {
      expect(mockCallCoreRpc).toHaveBeenCalledWith({
        method: 'security.get_usage',
        params: { scope: 'weekly' },
      });
    });
  });

  it('renders used / limit and percent after a successful fetch', async () => {
    mockCallCoreRpc.mockResolvedValue(sample({ used_usd: 0.4, limit_usd: 1.0, percent: 40 }));
    render(<BudgetGauge scope="daily" refreshIntervalMs={null} />);
    await waitFor(() => {
      expect(screen.getByTestId('budget-gauge-daily')).toBeTruthy();
    });
    expect(screen.getByText('$0.40')).toBeTruthy();
    expect(screen.getByText('/ $1.00')).toBeTruthy();
    const bar = screen.getByRole('progressbar');
    expect(bar.getAttribute('aria-valuenow')).toBe('40');
  });

  it('uses the warning color class at ≥ 80% usage', async () => {
    mockCallCoreRpc.mockResolvedValue(sample({ used_usd: 0.85, limit_usd: 1.0, percent: 85 }));
    render(<BudgetGauge scope="daily" refreshIntervalMs={null} />);
    const fill = await screen.findByTestId('budget-gauge-daily-fill');
    expect(fill.className).toContain('bg-coral');
  });

  it('uses the caution color class between 60% and 80%', async () => {
    mockCallCoreRpc.mockResolvedValue(sample({ used_usd: 0.7, limit_usd: 1.0, percent: 70 }));
    render(<BudgetGauge scope="daily" refreshIntervalMs={null} />);
    const fill = await screen.findByTestId('budget-gauge-daily-fill');
    expect(fill.className).toContain('bg-amber');
  });

  it('uses the calm color class below 60%', async () => {
    mockCallCoreRpc.mockResolvedValue(sample({ used_usd: 0.3, limit_usd: 1.0, percent: 30 }));
    render(<BudgetGauge scope="daily" refreshIntervalMs={null} />);
    const fill = await screen.findByTestId('budget-gauge-daily-fill');
    expect(fill.className).toContain('bg-sage');
  });

  it('clamps the visible fill width to 100% when percent exceeds it', async () => {
    mockCallCoreRpc.mockResolvedValue(sample({ used_usd: 1.5, limit_usd: 1.0, percent: 150 }));
    render(<BudgetGauge scope="daily" refreshIntervalMs={null} />);
    const fill = await screen.findByTestId('budget-gauge-daily-fill');
    expect(fill.getAttribute('style')).toContain('width: 100%');
  });

  it('shows an alert role when the RPC fails', async () => {
    mockCallCoreRpc.mockRejectedValue(new Error('offline'));
    render(<BudgetGauge scope="daily" refreshIntervalMs={null} />);
    await waitFor(() => {
      expect(screen.getByTestId('budget-gauge-error')).toBeTruthy();
    });
    expect(screen.getByRole('alert').textContent).toContain('offline');
  });

  it('reports the "한도 미설정" status when limit is zero', async () => {
    mockCallCoreRpc.mockResolvedValue(sample({ used_usd: 0, limit_usd: 0, percent: 0 }));
    render(<BudgetGauge scope="monthly" refreshIntervalMs={null} />);
    const status = await screen.findByTestId('budget-gauge-monthly-status');
    expect(status.textContent).toBe('한도 미설정');
  });

  it('refreshes on the configured interval', async () => {
    vi.useFakeTimers();
    mockCallCoreRpc.mockResolvedValue(sample());
    render(<BudgetGauge scope="daily" refreshIntervalMs={5_000} />);
    await vi.waitFor(() => {
      expect(mockCallCoreRpc).toHaveBeenCalledTimes(1);
    });
    await vi.advanceTimersByTimeAsync(5_000);
    expect(mockCallCoreRpc).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(5_000);
    expect(mockCallCoreRpc).toHaveBeenCalledTimes(3);
  });
});
