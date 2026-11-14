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
    [
      'リスト項目で開いたフェンスは項目の終わりで閉じる',
      '- ```\n  - [ ] code\n  ```\n\n- [x] real',
    ],
    ['引用で開いたフェンスは引用の終わりで閉じる', '> ```\n> - [ ] code\n\n- [x] real'],
    ['項目の中で開いて閉じないフェンス', '- a\n  ```\n  - [ ] x\n\n段落\n\n- [ ] b'],
    ['フェンスの中の引用の印は剥がさない', '```\n> ```\n- [ ] a\n```\n- [x] b'],
    ['引用の中のフェンス', '> ```\n> - [ ] a\n> ```\n> - [x] b'],
    ['リスト項目の先頭のコメント', '- <!--\n  - [ ] code\n  -->\n\n- [x] real'],
    ['リスト項目の 1 行コメント', '- <!-- - [ ] a -->\n- [x] b'],
    ['引用の中のコメントは引用の終わりで閉じる', '> <!--\n> - [ ] a\n\n- [x] b'],
    ['全角空白だけの本文', '- [ ] \u3000'],
    ['NBSP だけの本文', '- [ ] \u00a0'],
    ['空白だけの本文', '- [ ]  \n- [x] a'],
    ['同じ行の入れ子', '- - [ ] a'],
    ['番号付きの中の箇条書き', '1. - [x] a'],
    ['印の後ろ 5 空白のコード', '-     [ ] a'],
    ['4 桁字下げの閉じフェンス', '```\n- [ ] a\n    ```\n- [x] b\n```'],
    ['引用の中のリストのフェンス', '> - ```\n>   - [ ] a\n>   ```\n> - [x] b'],
    ['行頭の全角空白は字下げではない', '\u3000- [ ] a'],
    ['行頭の NBSP は字下げではない', '\u00a0- [x] a'],
    ['行頭の全角空白の段落の後', '段落\n\u3000- [ ] a'],
    ['全角空白付きの閉じフェンスは閉じない', '```\n- [ ] a\n\u3000```\n- [x] b\n```'],
    ['NBSP 付きの閉じフェンスは閉じない', '```\n- [ ] a\n\u00a0```\n- [x] b\n```'],
    ['全角空白だけの行はフェンスの中で空行ではない', '- ```\n\u3000\n  - [ ] a\n  ```\n- [x] b'],
    ['全角空白だけの行は項目を閉じない扱いにならない', '- a\n\u3000\n  - [x] b'],
    ['印の後ろの全角空白は空白ではない', '-\u3000[ ] a'],
  ];

  it.each(cases)('%s', async (_name, md) => {
    const html = await renderDescription(md);
    const boxes = html.match(/<input[^>]*type="checkbox"[^>]*>/g) ?? [];
    const total = boxes.length;
    const done = boxes.filter((tag) => /\bchecked\b/.test(tag)).length;
    expect(countChecklist(md) ?? { done: 0, total: 0 }).toEqual({ done, total });
  });
});

describe('入力の大きさ', () => {
  // 1 行に印を大量に並べても、数える時間が入力の長さの 2 乗にならない
  it.each([
    ['未チェックの印の列', `${'- '.repeat(49999)}[ ] a`],
    ['チェック済みの印の列', `${'- '.repeat(40000)}[x] a`],
    ['番号の印の列', `${'1. '.repeat(30000)}[ ] a`],
  ])('%s が短い時間で返り、件数が膨らまない', (_name, md) => {
    const start = performance.now();
    const result = countChecklist(md);
    expect(performance.now() - start).toBeLessThan(100);
    expect(result === null || result.total <= 1).toBe(true);
  });

  it('1 行の印が上限以内なら数える', () => {
    expect(countChecklist(`${'- '.repeat(50)}[x] a`)).toEqual({ done: 1, total: 1 });
  });

  it('印 100 個の行は数え、101 個の行は数えない（境目）', () => {
    expect(countChecklist(`${'- '.repeat(100)}[x] a`)).toEqual({ done: 1, total: 1 });
    expect(countChecklist(`${'- '.repeat(101)}[x] a`)).toBeNull();
  });

  it('印が上限を超えた行でも、後続の行の所属判定を保つ', () => {
    // 101 個の入れ子の項目の中でフェンスを開き、項目の外の次の行で閉じてから通常の項目が続く
    expect(countChecklist(`${'- '.repeat(101)}a\n  \`\`\`\n- [x] real\n- [ ] real2`)).toEqual({
      done: 1,
      total: 2,
    });
  });
});
