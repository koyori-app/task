import { afterEach, describe, expect, it } from 'vitest';

import { MAX_SELECTION_TITLE_LENGTH, selectionTitle } from '../selection';

function setup(html: string) {
  document.body.innerHTML = html;
  return document.body;
}

function select(start: Node, startOffset: number, end: Node, endOffset: number) {
  const selection = window.getSelection()!;
  selection.removeAllRanges();
  const range = document.createRange();
  range.setStart(start, startOffset);
  range.setEnd(end, endOffset);
  selection.addRange(range);
  return selection;
}

afterEach(() => {
  window.getSelection()?.removeAllRanges();
  document.body.innerHTML = '';
});

describe('selectionTitle', () => {
  it('本文内の選択を空白を畳んでタイトルにする', () => {
    const body = setup('<div id="doc"><p>ログイン画面の\n  エラー表示を直す</p></div>');
    const doc = body.querySelector('#doc')!;
    const text = doc.querySelector('p')!.firstChild!;
    const selection = select(text, 0, text, text.textContent!.length);

    expect(selectionTitle(selection, doc)).toBe('ログイン画面の エラー表示を直す');
  });

  it('本文の外に掛かる選択は使わない', () => {
    const body = setup('<div id="doc"><p>本文</p></div><p id="out">外の文</p>');
    const doc = body.querySelector('#doc')!;
    const selection = select(
      doc.querySelector('p')!.firstChild!,
      0,
      body.querySelector('#out')!.firstChild!,
      2,
    );

    expect(selectionTitle(selection, doc)).toBe('');
  });

  it('選択が無ければ空文字', () => {
    const body = setup('<div id="doc"><p>本文</p></div>');
    window.getSelection()!.removeAllRanges();

    expect(selectionTitle(window.getSelection(), body.querySelector('#doc'))).toBe('');
    expect(selectionTitle(null, body)).toBe('');
  });

  it('120 文字ちょうどはそのまま、121 文字は 120 文字に切る', () => {
    const exact = 'あ'.repeat(MAX_SELECTION_TITLE_LENGTH);
    const body = setup(`<div id="doc"><p>${exact}</p><p>${exact}い</p></div>`);
    const doc = body.querySelector('#doc')!;
    const [first, second] = [...doc.querySelectorAll('p')].map((p) => p.firstChild!);

    expect(selectionTitle(select(first!, 0, first!, exact.length), doc)).toBe(exact);
    expect(selectionTitle(select(second!, 0, second!, exact.length + 1), doc)).toBe(exact);
  });

  it('サロゲートペアを途中で割らない', () => {
    const emoji = '😀'.repeat(MAX_SELECTION_TITLE_LENGTH + 1);
    const body = setup(`<div id="doc"><p>${emoji}</p></div>`);
    const doc = body.querySelector('#doc')!;
    const text = doc.querySelector('p')!.firstChild!;

    const title = selectionTitle(select(text, 0, text, emoji.length), doc);
    expect(title).toBe('😀'.repeat(MAX_SELECTION_TITLE_LENGTH));
  });
});
