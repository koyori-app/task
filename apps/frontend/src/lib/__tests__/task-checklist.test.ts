import { describe, expect, it } from 'vitest';

import { KFM_STORY_INPUTS } from '@/lib/kfm-story-fixtures/inputs';
import { renderDescription } from '../markup-renderer';
import { countChecklist } from '../task-checklist';

describe('countChecklist', () => {
  it('null / undefined / 空 / タスクリストの無い本文は null', () => {
    expect(countChecklist(null)).toBeNull();
    expect(countChecklist(undefined)).toBeNull();
    expect(countChecklist('')).toBeNull();
    expect(countChecklist('ただの説明\n\n- 箇条書き\n- [リンク](https://example.com)')).toBeNull();
  });

  it('完了数と総数を数える', () => {
    expect(countChecklist('- [x] a\n- [ ] b\n- [ ] c')).toEqual({ done: 1, total: 3 });
  });

  it('大文字の [X] は完了', () => {
    expect(countChecklist('- [X] a\n- [ ] b')).toEqual({ done: 1, total: 2 });
  });

  it.each(['* [ ] a', '+ [x] a', '1. [ ] a', '2) [x] a'])('印の種類: %s を数える', (md) => {
    expect(countChecklist(md)?.total).toBe(1);
  });

  it('ネストした項目を親も子も 1 件ずつ数える', () => {
    expect(countChecklist(KFM_STORY_INPUTS['gfm-task-list'])).toEqual({ done: 2, total: 5 });
  });

  it('フェンスの中は数えず、閉じた後の項目は数える', () => {
    expect(countChecklist('```\n- [ ] a\n```\n- [x] b')).toEqual({ done: 1, total: 1 });
    expect(countChecklist('~~~\n- [ ] a\n~~~\n- [ ] b')).toEqual({ done: 0, total: 1 });
    expect(countChecklist('```\n- [ ] a\n- [x] b')).toBeNull();
  });

  it('HTML コメントの中は数えない', () => {
    expect(countChecklist('<!--\n- [ ] a\n-->\n- [x] b')).toEqual({ done: 1, total: 1 });
    expect(countChecklist('<!-- - [ ] a -->\n- [x] b')).toEqual({ done: 1, total: 1 });
  });

  it('引用の中は数える', () => {
    expect(countChecklist('> - [x] a')).toEqual({ done: 1, total: 1 });
  });

  it.each([
    '- [ ]foo',
    '- [] a',
    '- [xx] a',
    '- [x](https://example.com)',
    '段落\n[ ] a',
    '行中の `- [ ] a`',
  ])('数えない形: %s', (md) => {
    expect(countChecklist(md)).toBeNull();
  });

  it('字下げコードは数えず、リストの子として字下げした項目は数える', () => {
    expect(countChecklist('段落\n\n    - [ ] a')).toBeNull();
    expect(countChecklist('- 親\n    - [ ] 子')).toEqual({ done: 0, total: 1 });
  });

  it('CRLF の本文でも同じ結果', () => {
    expect(countChecklist('- [x] a\r\n- [ ] b\r\n```\r\n- [ ] c\r\n```')).toEqual({
      done: 1,
      total: 2,
    });
  });

  it('全部完了', () => {
    expect(countChecklist('- [x] a\n- [x] b')).toEqual({ done: 2, total: 2 });
  });
});

// 描画器が出すチェックボックスの数と一致することを確かめる。ずれたら描画器を正として直す
describe('描画器との一致', () => {
  const cases: Array<[string, string]> = [
    ['基本', '- [x] a\n- [ ] b\n- [ ] c'],
    ['大文字', '- [X] a\n- [ ] b'],
    ['* 印', '* [ ] a'],
    ['+ 印', '+ [x] a'],
    ['番号 .', '1. [ ] a'],
    ['番号 )', '2) [x] a'],
    ['ネスト', KFM_STORY_INPUTS['gfm-task-list']],
    ['バッククォートのフェンス', '```\n- [ ] a\n```\n- [x] b'],
    ['チルダのフェンス', '~~~\n- [ ] a\n~~~\n- [ ] b'],
    ['閉じないフェンス', '```\n- [ ] a\n- [x] b'],
    ['複数行コメント', '<!--\n- [ ] a\n-->\n- [x] b'],
    ['1 行コメント', '<!-- - [ ] a -->\n- [x] b'],
    ['引用', '> - [x] a'],
    ['直後に字が続く', '- [ ]foo'],
    ['空の括弧', '- [] a'],
    ['xx', '- [xx] a'],
    ['リンク', '- [x](https://example.com)'],
    ['印の無い段落', '段落\n[ ] a'],
    ['行中のコード', '行中の `- [ ] a`'],
    ['字下げコード', '段落\n\n    - [ ] a'],
    ['リストの子の字下げ', '- 親\n    - [ ] 子'],
    ['本文の無い項目', '- [ ]'],
    ['本文の無い項目と続き', '- [ ]\n- [x] a'],
  ];

  it.each(cases)('%s', async (_name, md) => {
    const html = await renderDescription(md);
    const boxes = html.match(/<input[^>]*type="checkbox"[^>]*>/g) ?? [];
    const total = boxes.length;
    const done = boxes.filter((tag) => /\bchecked\b/.test(tag)).length;
    expect(countChecklist(md) ?? { done: 0, total: 0 }).toEqual({ done, total });
  });
});
