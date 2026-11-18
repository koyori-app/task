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

  it('項目の先頭の `[x]:` が参照定義として消費される形では、実際のチェックの位置を付ける', async () => {
    // チェックは 2 行目 (構文木がチェックボックスにした方)。定義のラベルの `[x]` ではない
    for (const second of ['[ ] foo', '[x] foo']) {
      const src = `- [x]: http://e.com\n  ${second}\n\n参照: [リンク][x]`;
      const found = boxes(await renderDescription(src));
      expect(found, src).toHaveLength(1);
      expect(found[0].offset, src).toBe(src.indexOf(second));
      expect(found[0].checked, src).toBe(second.startsWith('[x]'));
    }
  });

  it('先頭の字が項目の印の直後の `[x]` でも、構文木のチェックでなければ付かない', async () => {
    // チェックの形 (`[x]:`) でなく、定義だけの項目。input が無い
    const html = await renderDescription('- [x]: http://e.com');
    expect(html).not.toContain('<input');
    expect(html).not.toContain('data-kfm-task-offset');
  });

  it('`]` の後ろの空白の数・タブ・行末のどの書き方でも位置が合う', async () => {
    const gaps = ['', ' ', '  ', '   ', '\t', ' \t', '\t ', ' \n  ', '  \n  ', '\t\n  ', '\n  '];
    const bodies = ['foo', '*foo*', '[a](http://e.com)', ' foo'];
    const prefixes = ['- ', '1. ', '> - ', '  - ', '*\t'];
    for (const prefix of prefixes) {
      for (const box of ['[ ]', '[x]', '[X]']) {
        for (const gap of gaps) {
          for (const body of bodies) {
            // 継続行は印の幅だけ字下げする (引用の中は `> ` を付ける)
            const cont = prefix.startsWith('>') ? '>   ' : '';
            const src = `${prefix}${box}${gap.replace(/\n {2}/g, `\n${cont}  `)}${body}`;
            const found = boxes(await renderDescription(src));
            // `]` の直後が空白・タブ・行末のときだけチェックになる (`[x]foo` はならない)
            const isCheck = /^[ \t\n]/.test(`${gap}${body}`);
            if (!isCheck) {
              expect(found, JSON.stringify(src)).toHaveLength(0);
              continue;
            }
            expect(found, JSON.stringify(src)).toHaveLength(1);
            expect(src.slice(found[0].offset, found[0].offset + 3), JSON.stringify(src)).toBe(box);
          }
        }
      }
    }
  });

  it('`]` の後ろが空白 1 字で行が終わり、本文が次の行にある項目にも位置が付く', async () => {
    for (const src of ['- [x] \n  foo', '- [ ] \n  foo', '- [x]\t\n  foo', '> - [ ] \n>   foo']) {
      const found = boxes(await renderDescription(src));
      expect(found, JSON.stringify(src)).toHaveLength(1);
      expect(found[0].offset, JSON.stringify(src)).toBe(src.indexOf('['));
      expect(src.slice(found[0].offset, found[0].offset + 3)).toMatch(/^\[[ x]\]$/);
    }
  });

  it('チェックの後が装飾・入れ子・空でも位置が合う', async () => {
    const cases = [
      '- [ ] *強調*',
      '- [x] **太字** a',
      '- [ ] [リンク](http://e.com)',
      '- [x] `コード`',
      '- [ ] [ ] 二重',
      '- [x]\n  本文',
      '- [ ] a\n  - [x] b',
    ];
    for (const src of cases) {
      const found = boxes(await renderDescription(src));
      expect(found.length, src).toBeGreaterThan(0);
      for (const { offset, checked } of found) {
        expect(src.slice(offset, offset + 3), src).toBe(checked ? '[x]' : '[ ]');
      }
    }
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
      '- [x]: http://e.com',
      '  [ ] foo',
      '  [x] foo',
      '参照: [リンク][x]',
      '- [ ] *強調*',
      '- [x]',
      '- [x] ',
      '  foo',
      '- [ ]\t',
      '   - [ ] 字下げ',
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

    // 時間切れの上限は 30 秒にする（vitest の既定は 5 秒）。
    // 何のために: 300 通りの本文を描画し直すので重い。単独で撃つと約 3.6〜4.4 秒で通るが、
    // pre-push の hook で frontend の全単体試験を並べて撃つと、約 6 秒（5987ms）かかって既定の上限で落ちた。
    // 同じ並び撃ちを Coder で撃ち直すと、load average が 18 まで上がり 10.4 秒かかった。
    // 試験の誤りではなく負荷で延びるので、上限だけを上げる。
    // なぜ 30 秒か: 並び撃ちで見た最も遅い値（10.4 秒）の倍より上に置く。他の重い試験（CommentBody など）の上限とも揃う。
    // 何が起きたら解くか: 描画（renderDescription）が速くなり、並び撃ちでも 5 秒を十分下回るようになったら、既定に戻す。
    it('属性を持つ全 input で、字・切り替え・再描画が一致する', async () => {
      const rand = mulberry32(539);
      let checkedTotal = 0;
      for (let n = 0; n < 300; n += 1) {
        const src = generate(rand);
        const html = await renderDescription(src);
        const found = boxes(html);
        // 部品は位置の付く形だけなので、描画された全チェックボックスに位置が付く
        expect(found.length, JSON.stringify(src)).toBe((html.match(/<input /g) ?? []).length);
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
    }, 30_000);
  });
});
