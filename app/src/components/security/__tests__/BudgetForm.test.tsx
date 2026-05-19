import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import BudgetForm from '../BudgetForm';

const { mockCallCoreRpc } = vi.hoisted(() => ({
  mockCallCoreRpc: vi.fn(),
}));

vi.mock('../../../services/coreRpcClient', () => ({
  callCoreRpc: mockCallCoreRpc,
}));

describe('BudgetForm', () => {
  beforeEach(() => {
    mockCallCoreRpc.mockReset();
  });

  it('disables save until the user enters a limit', () => {
    render(<BudgetForm scope="daily" />);
    expect(screen.getByRole('button', { name: '저장' })).toBeDisabled();
  });

  it('submits security.set_budget with the daily scope and parsed limit', async () => {
    const user = userEvent.setup();
    mockCallCoreRpc.mockResolvedValue({
      budget_id: 'b1',
      scope: 'daily',
      limit_usd: 10,
    });
    render(<BudgetForm scope="daily" />);
    await user.type(screen.getByLabelText(/일일 한도/), '10');
    await user.click(screen.getByRole('button', { name: '저장' }));
    await waitFor(() => {
      expect(mockCallCoreRpc).toHaveBeenCalledWith({
        method: 'security.set_budget',
        params: { scope: 'daily', limit_usd: 10 },
      });
    });
    expect(screen.getByTestId('budget-form-daily-success').textContent).toContain('$10.00');
  });

  it('rejects negative limits client-side without calling the core', async () => {
    const user = userEvent.setup();
    render(<BudgetForm scope="weekly" />);
    const input = screen.getByLabelText(/주간 한도/);
    await user.type(input, '-3');
    await user.click(screen.getByRole('button', { name: '저장' }));
    expect(mockCallCoreRpc).not.toHaveBeenCalled();
    expect(screen.getByTestId('budget-form-weekly-error').textContent).toContain('0 이상');
  });

  it('invokes the onSaved callback after a successful save', async () => {
    const user = userEvent.setup();
    const onSaved = vi.fn();
    mockCallCoreRpc.mockResolvedValue({
      budget_id: 'b2',
      scope: 'monthly',
      limit_usd: 200,
    });
    render(<BudgetForm scope="monthly" onSaved={onSaved} />);
    await user.type(screen.getByLabelText(/월간 한도/), '200');
    await user.click(screen.getByRole('button', { name: '저장' }));
    await waitFor(() => {
      expect(onSaved).toHaveBeenCalledWith({
        budget_id: 'b2',
        scope: 'monthly',
        limit_usd: 200,
      });
    });
  });

  it('shows an error alert and does not show success when the RPC rejects', async () => {
    const user = userEvent.setup();
    mockCallCoreRpc.mockRejectedValue(new Error('core offline'));
    render(<BudgetForm scope="daily" />);
    await user.type(screen.getByLabelText(/일일 한도/), '5');
    await user.click(screen.getByRole('button', { name: '저장' }));
    await waitFor(() => {
      expect(screen.getByTestId('budget-form-daily-error').textContent).toContain('core offline');
    });
    expect(screen.queryByTestId('budget-form-daily-success')).toBeNull();
  });

  it('disables save while the RPC is in flight', async () => {
    const user = userEvent.setup();
    let resolve: ((value: unknown) => void) | undefined;
    mockCallCoreRpc.mockReturnValue(
      new Promise(res => {
        resolve = res;
      }),
    );
    render(<BudgetForm scope="daily" />);
    await user.type(screen.getByLabelText(/일일 한도/), '1');
    await user.click(screen.getByRole('button', { name: '저장' }));
    await waitFor(() => {
      expect(screen.getByRole('button', { name: '저장 중…' })).toBeDisabled();
    });
    resolve?.({ budget_id: 'b3', scope: 'daily', limit_usd: 1 });
    await waitFor(() => {
      expect(screen.getByRole('button', { name: '저장' })).not.toBeDisabled();
    });
  });
});
