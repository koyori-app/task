import { describe, expect, it, vi } from 'vitest';
import { handleRowKeydownNavigate, isRowInteractiveTarget } from '@/lib/table-row-navigation';

function mountRow(html: string) {
  document.body.innerHTML = html;
  return document.getElementById('row')!;
}

describe('isRowInteractiveTarget', () => {
  it('returns true for checkbox descendants', () => {
    mountRow(`
      <tr id="row" role="button">
        <td><button role="checkbox" id="cb" type="button"></button></td>
      </tr>
    `);
    const checkbox = document.getElementById('cb')!;
    const event = new KeyboardEvent('keydown', { key: ' ', bubbles: true });
    checkbox.dispatchEvent(event);
    expect(isRowInteractiveTarget(event)).toBe(true);
  });

  it('returns true for anchor descendants', () => {
    mountRow(`
      <tr id="row" role="button">
        <td><a href="/detail" id="link">title</a></td>
      </tr>
    `);
    const link = document.getElementById('link')!;
    const event = new KeyboardEvent('keydown', { key: 'Enter', bubbles: true });
    link.dispatchEvent(event);
    expect(isRowInteractiveTarget(event)).toBe(true);
  });

  it('returns false for non-interactive row body', () => {
    mountRow(`
      <tr id="row" role="button">
        <td><span id="status">In Progress</span></td>
      </tr>
    `);
    const status = document.getElementById('status')!;
    const event = new KeyboardEvent('keydown', { key: 'Enter', bubbles: true });
    status.dispatchEvent(event);
    expect(isRowInteractiveTarget(event)).toBe(false);
  });
});

describe('handleRowKeydownNavigate', () => {
  it('does not navigate when Space originates from checkbox', () => {
    mountRow(`
      <tr id="row" role="button">
        <td><button role="checkbox" id="cb" type="button"></button></td>
      </tr>
    `);
    const checkbox = document.getElementById('cb')!;
    const navigate = vi.fn();
    const event = new KeyboardEvent('keydown', { key: ' ', bubbles: true, cancelable: true });
    checkbox.dispatchEvent(event);
    handleRowKeydownNavigate(event, navigate);
    expect(navigate).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(false);
  });

  it('navigates on Enter from row body', () => {
    mountRow(`
      <tr id="row" role="button">
        <td><span id="status">In Progress</span></td>
      </tr>
    `);
    const status = document.getElementById('status')!;
    const navigate = vi.fn();
    const event = new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true });
    status.dispatchEvent(event);
    handleRowKeydownNavigate(event, navigate);
    expect(navigate).toHaveBeenCalledOnce();
    expect(event.defaultPrevented).toBe(true);
  });

  it('navigates on Space from row body', () => {
    mountRow(`
      <tr id="row" role="button">
        <td><span id="status">In Progress</span></td>
      </tr>
    `);
    const status = document.getElementById('status')!;
    const navigate = vi.fn();
    const event = new KeyboardEvent('keydown', { key: ' ', bubbles: true, cancelable: true });
    status.dispatchEvent(event);
    handleRowKeydownNavigate(event, navigate);
    expect(navigate).toHaveBeenCalledOnce();
    expect(event.defaultPrevented).toBe(true);
  });
});
