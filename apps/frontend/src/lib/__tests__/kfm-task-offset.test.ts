// タスクリストの input に付く原文の位置 (data-kfm-task-offset) と、切り替えの書き換え関数との突き合わせ
import { describe, expect, it } from 'vitest';
import { renderDescription } from '../markup-renderer';
import { setChecklistHtmlChecked, toggleChecklistAt } from '../task-checklist-toggle';

type Box = { offset: number; checked: boolean };

/** 描画 HTML の `input` から、位置と checked を出現順に取り出す */
function boxes(html: string): Box[] {
  return [...html.matchAll(/<input [^>]*>/g)]
    .map((m) => m[0])
    .filter((tag) => tag.includes('data-kfm-task-offset='))
    .map((tag) => ({
      offset: Number(/data-kfm-task-offset="(\d+)"/.exec(tag)?.[1]),
      checked: tag.includes(' checked=""'),
    }));
}

const lf = (s: string) => s.replace(/\r\n?/g, '\n');

describe('rehype-kfm-task-offset (原文の位置)', () => {
  it('各 input に、原文の `[` の添字を付ける', async () => {
    const src = '- [x] 済み\n- [ ] 未了';
    const html = await renderDescription(src);
    expect(boxes(html)).toEqual([
      { offset: 2, checked: true },
      { offset: 11, checked: false },
    ]);
    expect(src.slice(2, 5)).toBe('[x]');
    expect(src.slice(11, 14)).toBe('[ ]');
  });

  it('入れ子・番号付き・引用の中・空行で離したリストでも、添字の 3 字が `[ ]` か `[x]`', async () => {
    const cases = [
      '- [ ] 親\n  - [x] 子\n    - [ ] 孫',
      '1. [ ] a\n2) [X] b',
      '> - [ ] a\n> - [x] b',
      '- [ ] a\n\n- [x] b\n\n- [ ] c',
      '- [ ] a\n\n  - [x] 子',
      '*\t[ ] タブ区切り\n+ [x] プラス',
      '- - [ ] e',
    ];
    for (const src of cases) {
      const found = boxes(await renderDescription(src));
      expect(found.length, src).toBeGreaterThan(0);
      for (const { offset, checked } of found) {
        const three = src.slice(offset, offset + 3);
        expect(three, src).toMatch(checked ? /^\[[xX]\]$/ : /^\[[ \t]\]$/);
      }
    }
  });

  it('空行で離したリスト (input が p の中) にも付く', async () => {
    const html = await renderDescription('- [ ] a\n\n- [x] b');
    expect(html).toMatch(/<li class="task-list-item">\s*<p><input/);
    expect(boxes(html)).toHaveLength(2);
  });

  it('CRLF の本文は LF の本文と同じ HTML (添字は LF 換算)', async () => {
    const a = await renderDescription('- [x] a\r\n- [ ] b\r\n- [ ] c');
    const b = await renderDescription('- [x] a\n- [ ] b\n- [ ] c');
    expect(a).toBe(b);
    expect(boxes(a).map((x) => x.offset)).toEqual([2, 10, 18]);
  });

  it('コードフェンスの中の `- [ ] a` には input が出ない', async () => {
    const html = await renderDescription('```\n- [ ] a\n```');
    expect(html).not.toContain('<input');
    expect(html).not.toContain('data-kfm-task-offset');
  });

  it('属性を付けても disabled は残る', async () => {
    const html = await renderDescription('- [ ] a');
    expect(html).toContain('<input type="checkbox" disabled="" data-kfm-task-offset="2">');
  });

  it('タスクリストでない本文には属性が出ない', async () => {
    expect(await renderDescription('- a\n- b\n\n[ ] c')).not.toContain('data-kfm-task-offset');
  });

  it('位置が合わない項目 (`[` の前で改行する形) には何も付けず、押せないまま残す', async () => {
    const html = await renderDescription('- [x] ok\n- [\n] a');
    expect(boxes(html)).toEqual([{ offset: 2, checked: true }]);
  });

  describe('切り替え関数との突き合わせ (種を固定した乱数)', () => {
    const PARTS = [
      '- [ ] a',
      '  - [x] b',
      '1. [X] c',
      '> - [ ] d',
      '',
      '```',
      '<!-- -->',
      '- - [ ] e',
      '* [x]',
      '地の文',
      '\t- [ ] tab',
      '- [x] 末尾\\',
    ];

    function mulberry32(seed: number) {
      let a = seed;
      return () => {
        a = (a + 0x6d2b79f5) | 0;
        let t = Math.imul(a ^ (a >>> 15), 1 | a);
        t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
        return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
      };
    }

    function generate(rand: () => number): string {
      const count = 1 + Math.floor(rand() * 8);
      const lines: string[] = [];
      for (let i = 0; i < count; i += 1) lines.push(PARTS[Math.floor(rand() * PARTS.length)]);
      return lines.join(rand() < 0.3 ? '\r\n' : '\n');
    }

    it('属性を持つ全 input で、字・切り替え・再描画が一致する', async () => {
      const rand = mulberry32(539);
      let checkedTotal = 0;
      for (let n = 0; n < 300; n += 1) {
        const src = generate(rand);
        const html = await renderDescription(src);
        const found = boxes(html);
        // 位置は input ごとに重ならない
        expect(new Set(found.map((b) => b.offset)).size, src).toBe(found.length);
        for (const { offset, checked } of found) {
          checkedTotal += 1;
          // checked 属性の有無が原文の字と一致する
          const raw = lf(src)[offset + 1];
          expect(checked, JSON.stringify(src)).toBe(raw === 'x' || raw === 'X');
          for (const want of [!checked, checked]) {
            const next = toggleChecklistAt(src, offset, want);
            expect(next, JSON.stringify(src)).not.toBeNull();
            // 改行コードは変わらない
            expect(next!.length).toBe(src.length);
            // 書き換えた HTML はサーバが新しい本文を描いた HTML と 1 字も違わない
            expect(setChecklistHtmlChecked(html, offset, want), JSON.stringify(src)).toBe(
              await renderDescription(next!),
            );
          }
        }
      }
      // 乱数の部品が実際にチェックボックスを出していること (空振りの検査にしない)
      expect(checkedTotal).toBeGreaterThan(300);
    });
  });
});
