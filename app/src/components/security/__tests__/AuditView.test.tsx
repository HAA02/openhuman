import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import AuditView from '../AuditView';

const { mockCallCoreRpc } = vi.hoisted(() => ({ mockCallCoreRpc: vi.fn() }));

vi.mock('../../../services/coreRpcClient', () => ({ callCoreRpc: mockCallCoreRpc }));

const samplePayload = (
  overrides: Partial<{ records: object[]; has_more: boolean; source: string }> = {}
) => ({
  records: overrides.records ?? [
    {
      timestamp: '2026-05-19T03:24:00+00:00',
      kind: 'LlmCallPost',
      status: 'Success',
      channel: 'chat',
      cost_usd: 0.0123,
      model: 'claude-sonnet-4-6',
    },
  ],
  has_more: overrides.has_more ?? false,
  source: overrides.source ?? '/home/user/.openhuman/audit.log',
});

describe('AuditView', () => {
  beforeEach(() => {
    mockCallCoreRpc.mockReset();
  });

  it('issues an initial security.get_audit fetch on mount', async () => {
    mockCallCoreRpc.mockResolvedValue(samplePayload());
    render(<AuditView limit={25} />);
    await waitFor(() => {
      expect(mockCallCoreRpc).toHaveBeenCalledWith({
        method: 'security.get_audit',
        params: { since: undefined, limit: 25 },
      });
    });
  });

  it('renders a row per record after a successful fetch', async () => {
    mockCallCoreRpc.mockResolvedValue(samplePayload());
    render(<AuditView />);
    await waitFor(() => {
      expect(screen.getAllByTestId('audit-view-row')).toHaveLength(1);
    });
    expect(screen.getByTestId('audit-view-source').textContent).toContain('audit.log');
  });

  it('shows the empty state when no records are returned', async () => {
    mockCallCoreRpc.mockResolvedValue(samplePayload({ records: [] }));
    render(<AuditView />);
    await waitFor(() => {
      expect(screen.getByTestId('audit-view-empty')).toBeTruthy();
    });
  });

  it('forwards the typed `since` filter as an RFC3339 string', async () => {
    const user = userEvent.setup();
    mockCallCoreRpc.mockResolvedValue(samplePayload());
    render(<AuditView />);
    await waitFor(() => {
      expect(mockCallCoreRpc).toHaveBeenCalledTimes(1);
    });
    await user.type(screen.getByLabelText(/이 시각 이후/), '2026-05-19T00:00:00Z');
    await user.click(screen.getByTestId('audit-view-refresh'));
    await waitFor(() => {
      expect(mockCallCoreRpc).toHaveBeenLastCalledWith({
        method: 'security.get_audit',
        params: { since: '2026-05-19T00:00:00Z', limit: 50 },
      });
    });
  });

  it('reveals the "has_more" hint when the core indicates more records exist', async () => {
    mockCallCoreRpc.mockResolvedValue(samplePayload({ has_more: true }));
    render(<AuditView />);
    await waitFor(() => {
      expect(screen.getByTestId('audit-view-has-more')).toBeTruthy();
    });
  });

  it('renders an error state when the RPC throws', async () => {
    mockCallCoreRpc.mockRejectedValue(new Error('audit log missing'));
    render(<AuditView />);
    await waitFor(() => {
      expect(screen.getByTestId('audit-view-error').textContent).toContain('audit log missing');
    });
  });
});
