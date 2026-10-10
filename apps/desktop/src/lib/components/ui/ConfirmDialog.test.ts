import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import ConfirmDialog from './ConfirmDialog.svelte';

describe('ConfirmDialog', () => {
  it('requires the confirmation text before confirming', async () => {
    const onConfirm = vi.fn();
    render(ConfirmDialog, {
      open: true,
      title: 'Erase the entire flash?',
      description: 'Everything will be erased.',
      confirmLabel: 'Erase entire flash',
      requireText: 'ERASE',
      onConfirm,
    });
    const user = userEvent.setup();
    const confirm = await screen.findByRole('button', { name: 'Erase entire flash' });
    expect(confirm).toBeDisabled();
    await user.type(screen.getByLabelText(/to confirm/), 'erase');
    expect(confirm).toBeDisabled();
    await user.clear(screen.getByLabelText(/to confirm/));
    await user.type(screen.getByLabelText(/to confirm/), 'ERASE');
    expect(confirm).toBeEnabled();
    await user.click(confirm);
    expect(onConfirm).toHaveBeenCalledOnce();
  });

  it('focuses Cancel and does not confirm when cancelled', async () => {
    const onConfirm = vi.fn();
    render(ConfirmDialog, { open: true, title: 'Write?', description: 'd', onConfirm });
    const cancel = await screen.findByRole('button', { name: 'Cancel' });
    await vi.waitFor(() => expect(cancel).toHaveFocus());
    await userEvent.setup().click(cancel);
    expect(onConfirm).not.toHaveBeenCalled();
  });
});
