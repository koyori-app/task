import { afterEach, describe, expect, it } from 'vitest';

import {
  shouldKeepOverlayOnEscape,
  shouldKeepOverlayOnPointerDownOutside,
} from '../overlay-dismiss-guard';

function renderOverlay() {
  document.body.innerHTML = `
    <dialog open>
      <button id="button">親タスク</button>
      <textarea id="draft"></textarea>
      <dialog id="confirm">
        <button id="confirm-cancel">キャンセル</button>
        <input id="confirm-input" />
      </dialog>
    </dialog>
  `;
}

function get<T extends Element = HTMLElement>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`element not found: ${selector}`);
  return element;
}

/** 指定要素を target にした Esc の keydown を作る */
function escapeOn(selector: string, init: KeyboardEventInit = {}) {
  const event = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, ...init });
  get(selector).dispatchEvent(event);
  return event;
}

afterEach(() => {
  document.body.innerHTML = '';
});

describe('shouldKeepOverlayOnEscape', () => {
  it('通常のボタンでの Esc は閉じる', () => {
    renderOverlay();
    expect(shouldKeepOverlayOnEscape(escapeOn('#button'))).toBe(false);
  });

  it('IME 変換中（isComposing）の Esc は閉じない', () => {
    renderOverlay();
    expect(shouldKeepOverlayOnEscape(escapeOn('#button', { isComposing: true }))).toBe(true);
  });

  it('IME 変換中（keyCode 229）の Esc は閉じない', () => {
    renderOverlay();
    expect(shouldKeepOverlayOnEscape(escapeOn('#button', { keyCode: 229 }))).toBe(true);
  });

  // 器には cancel が届かないので、止めると手前の dialog の Esc まで殺してしまう
  it('入れ子の <dialog> の中の Esc は止めない（値のある入力欄でも）', () => {
    renderOverlay();
    get('#confirm').setAttribute('open', '');
    get<HTMLInputElement>('#confirm-input').value = '入力中';
    expect(shouldKeepOverlayOnEscape(escapeOn('#confirm-cancel'))).toBe(false);
    expect(shouldKeepOverlayOnEscape(escapeOn('#confirm-input'))).toBe(false);
  });

  it('値のある入力欄での Esc は閉じない（下書きを残す）', () => {
    renderOverlay();
    get<HTMLTextAreaElement>('#draft').value = '書きかけのコメント';
    expect(shouldKeepOverlayOnEscape(escapeOn('#draft'))).toBe(true);
  });

  it('空の入力欄での Esc は閉じる', () => {
    renderOverlay();
    expect(shouldKeepOverlayOnEscape(escapeOn('#draft'))).toBe(false);
  });
});

describe('shouldKeepOverlayOnPointerDownOutside', () => {
  it('値のある入力欄にフォーカスがあれば閉じない', () => {
    renderOverlay();
    const draft = get<HTMLTextAreaElement>('#draft');
    draft.value = '書きかけのコメント';
    expect(shouldKeepOverlayOnPointerDownOutside(draft)).toBe(true);
  });

  it('空の入力欄にフォーカスがあれば閉じる', () => {
    renderOverlay();
    expect(shouldKeepOverlayOnPointerDownOutside(get('#draft'))).toBe(false);
  });

  it('フォーカスが無ければ閉じる', () => {
    expect(shouldKeepOverlayOnPointerDownOutside(null)).toBe(false);
  });
});
