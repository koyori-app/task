import { afterEach, describe, expect, it, vi } from 'vitest';
import { defineComponent, h } from 'vue';
import { enableAutoUnmount, mount } from '@vue/test-utils';
import { useKeyboardShortcuts } from '../useKeyboardShortcuts';

enableAutoUnmount(afterEach);
afterEach(() => {
  document.body.innerHTML = '';
});

function setup() {
  const handler = vi.fn();
  const wrapper = mount(
    defineComponent({
      setup() {
        useKeyboardShortcuts({ t: handler });
        return () => h('div');
      },
    }),
    { attachTo: document.body },
  );
  return { handler, wrapper };
}

function press(init: KeyboardEventInit & { keyCode?: number } = {}, target: EventTarget = window) {
  const event = new KeyboardEvent('keydown', {
    key: 't',
    bubbles: true,
    cancelable: true,
    ...init,
  });
  if (init.keyCode !== undefined) Object.defineProperty(event, 'keyCode', { value: init.keyCode });
  target.dispatchEvent(event);
  return event;
}

describe('useKeyboardShortcuts', () => {
  it('t で handler を呼び、既定の動作を止める', () => {
    const { handler } = setup();
    const event = press();
    expect(handler).toHaveBeenCalledOnce();
    expect(event.defaultPrevented).toBe(true);
  });

  it('Shift の有無は区別しない。割り当ての無いキーは止めない', () => {
    const { handler } = setup();
    press({ key: 'T', shiftKey: true });
    expect(handler).toHaveBeenCalledOnce();
    expect(press({ key: 'x' }).defaultPrevented).toBe(false);
  });

  it.each([
    ['input', '<input>'],
    ['textarea', '<textarea></textarea>'],
    ['select', '<select></select>'],
    ['contenteditable', '<div contenteditable="true"><span></span></div>'],
  ])('%s の中で押しても呼ばない', (_, html) => {
    const { handler } = setup();
    const host = document.createElement('div');
    host.innerHTML = html;
    document.body.append(host);
    const target = host.querySelector('span') ?? host.firstElementChild!;
    const event = press({}, target);
    expect(handler).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(false);
  });

  it('contenteditable="false" の中では呼ぶ', () => {
    const { handler } = setup();
    const host = document.createElement('div');
    host.setAttribute('contenteditable', 'false');
    document.body.append(host);
    press({}, host);
    expect(handler).toHaveBeenCalledOnce();
  });

  it.each([
    ['IME 変換中', { isComposing: true }],
    ['keyCode 229', { keyCode: 229 }],
    ['⌘', { metaKey: true }],
    ['Ctrl', { ctrlKey: true }],
    ['Alt', { altKey: true }],
    ['押しっぱなし', { repeat: true }],
  ])('%s なら呼ばない', (_, init) => {
    const { handler } = setup();
    press(init);
    expect(handler).not.toHaveBeenCalled();
  });

  it('ダイアログが開いていれば呼ばない', () => {
    const { handler } = setup();
    const dialog = document.createElement('div');
    dialog.setAttribute('role', 'dialog');
    document.body.append(dialog);
    press();
    expect(handler).not.toHaveBeenCalled();
    dialog.remove();
    press();
    expect(handler).toHaveBeenCalledOnce();
  });

  it('unmount 後は呼ばない', () => {
    const { handler, wrapper } = setup();
    wrapper.unmount();
    press();
    expect(handler).not.toHaveBeenCalled();
  });
});
