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

function mountShortcuts(bindings: Record<string, () => void>, options?: { fallback?: boolean }) {
  return mount(
    defineComponent({
      setup() {
        useKeyboardShortcuts(bindings, options);
        return () => h('div');
      },
    }),
    { attachTo: document.body },
  );
}

describe('useKeyboardShortcuts の fallback', () => {
  // クライアント遷移の後はレイアウト（fallback）が先に mount されるので、両方の順を見る
  it.each([
    ['画面が先', false],
    ['fallback が先', true],
  ])('同じキーを画面も持つときは画面だけを呼ぶ（%s）', (_, fallbackFirst) => {
    const primary = vi.fn();
    const fallback = vi.fn();
    if (fallbackFirst) mountShortcuts({ t: fallback }, { fallback: true });
    mountShortcuts({ t: primary });
    if (!fallbackFirst) mountShortcuts({ t: fallback }, { fallback: true });
    press();
    expect(primary).toHaveBeenCalledOnce();
    expect(fallback).not.toHaveBeenCalled();
  });

  it('画面の handler が何もしなくても fallback は呼ばない', () => {
    const fallback = vi.fn();
    mountShortcuts({ t: fallback }, { fallback: true });
    mountShortcuts({ t: () => {} });
    press();
    expect(fallback).not.toHaveBeenCalled();
  });

  it('画面を unmount すると fallback を呼ぶ', () => {
    const fallback = vi.fn();
    mountShortcuts({ t: fallback }, { fallback: true });
    const page = mountShortcuts({ t: vi.fn() });
    page.unmount();
    press();
    expect(fallback).toHaveBeenCalledOnce();
  });

  it('fallback だけなら呼び、既定の動作を止める', () => {
    const fallback = vi.fn();
    mountShortcuts({ t: fallback }, { fallback: true });
    const event = press();
    expect(fallback).toHaveBeenCalledOnce();
    expect(event.defaultPrevented).toBe(true);
  });

  it('画面が別のキーだけを持つときは fallback を呼ぶ', () => {
    const fallback = vi.fn();
    const other = vi.fn();
    mountShortcuts({ t: fallback }, { fallback: true });
    mountShortcuts({ n: other });
    press();
    expect(fallback).toHaveBeenCalledOnce();
    expect(other).not.toHaveBeenCalled();
  });
});
