import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import ActionButtonFixture from '../../../test/fixtures/ActionButtonFixture.svelte';

describe('ActionButton', () => {
  it('runs the action when available', async () => {
    const onclick = vi.fn();
    render(ActionButtonFixture, { onclick, disabledReason: null });
    await userEvent.setup().click(screen.getByRole('button', { name: 'Flash' }));
    expect(onclick).toHaveBeenCalledOnce();
  });

  it('explains and blocks the action when unavailable', async () => {
    const onclick = vi.fn();
    render(ActionButtonFixture, { onclick, disabledReason: 'Connect a device first.' });
    const button = screen.getByRole('button', { name: 'Flash' });
    expect(button).toHaveAttribute('aria-disabled', 'true');
    expect(button).toHaveAccessibleDescription('Connect a device first.');
    await userEvent.setup().click(button);
    expect(onclick).not.toHaveBeenCalled();
  });
});

