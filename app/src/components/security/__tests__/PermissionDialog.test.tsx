import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import PermissionDialog, { Permission } from '../PermissionDialog';

const allRequested: Permission[] = ['file_read', 'file_write', 'network'];

describe('PermissionDialog', () => {
  it('renders nothing when closed', () => {
    const { container } = render(
      <PermissionDialog
        open={false}
        skillName="x"
        requested={allRequested}
        onGrant={vi.fn()}
        onDeny={vi.fn()}
      />,
    );
    expect(container.firstChild).toBeNull();
  });

  it('shows the requested permissions when open', () => {
    render(
      <PermissionDialog
        open
        skillName="my-skill"
        requested={allRequested}
        onGrant={vi.fn()}
        onDeny={vi.fn()}
      />,
    );
    expect(screen.getByTestId('permission-dialog-skill').textContent).toBe('my-skill');
    expect(screen.getAllByRole('checkbox')).toHaveLength(allRequested.length);
  });

  it('defaults all requested permissions to checked', () => {
    render(
      <PermissionDialog
        open
        skillName="x"
        requested={allRequested}
        onGrant={vi.fn()}
        onDeny={vi.fn()}
      />,
    );
    for (const permission of allRequested) {
      const box = screen.getByTestId(`permission-dialog-check-${permission}`) as HTMLInputElement;
      expect(box.checked).toBe(true);
    }
    expect(screen.getByTestId('permission-dialog-grant').textContent).toContain('(3)');
  });

  it('emits the selected subset on grant when the user unchecks one', async () => {
    const user = userEvent.setup();
    const onGrant = vi.fn();
    render(
      <PermissionDialog
        open
        skillName="x"
        requested={allRequested}
        onGrant={onGrant}
        onDeny={vi.fn()}
      />,
    );
    await user.click(screen.getByTestId('permission-dialog-check-network'));
    await user.click(screen.getByTestId('permission-dialog-grant'));
    expect(onGrant).toHaveBeenCalledTimes(1);
    const granted = onGrant.mock.calls[0]?.[0] as Permission[];
    expect(granted).toContain('file_read');
    expect(granted).toContain('file_write');
    expect(granted).not.toContain('network');
  });

  it('fires onDeny without modifying selections', async () => {
    const user = userEvent.setup();
    const onDeny = vi.fn();
    const onGrant = vi.fn();
    render(
      <PermissionDialog
        open
        skillName="x"
        requested={allRequested}
        onGrant={onGrant}
        onDeny={onDeny}
      />,
    );
    await user.click(screen.getByTestId('permission-dialog-deny'));
    expect(onDeny).toHaveBeenCalledTimes(1);
    expect(onGrant).not.toHaveBeenCalled();
  });

  it('resets selection when the requested set changes between openings', () => {
    const { rerender } = render(
      <PermissionDialog
        open
        skillName="x"
        requested={['file_read']}
        onGrant={vi.fn()}
        onDeny={vi.fn()}
      />,
    );
    expect(screen.getByTestId('permission-dialog-grant').textContent).toContain('(1)');
    rerender(
      <PermissionDialog
        open
        skillName="x"
        requested={['file_read', 'network', 'system_info']}
        onGrant={vi.fn()}
        onDeny={vi.fn()}
      />,
    );
    expect(screen.getByTestId('permission-dialog-grant').textContent).toContain('(3)');
  });

  it('sets role="dialog" and aria-modal for keyboard accessibility', () => {
    render(
      <PermissionDialog
        open
        skillName="x"
        requested={['file_read']}
        onGrant={vi.fn()}
        onDeny={vi.fn()}
      />,
    );
    const dialog = screen.getByRole('dialog');
    expect(dialog.getAttribute('aria-modal')).toBe('true');
    expect(dialog.getAttribute('aria-label')).toContain('x');
  });
});
