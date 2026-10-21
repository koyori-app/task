import type { Paragraph, Root } from 'mdast';
import remarkParse from 'remark-parse';
import { unified } from 'unified';
import { describe, expect, it } from 'vitest';

import { renderDescription } from '@/lib/markup-renderer';
import { remarkKfmRawHtml } from '@/lib/remark-kfm-raw-html';

/**
 * markdown 中の生の HTML（行中の tag・HTML の行で始まる段落・HTML のコメント）は、落とさず字として出す。
 * 説明文（github profile）とコメント（comment profile）の両方で同じ。
 * 生の HTML を HTML として通す経路は無い（allowlist の tag は無い）ので、見える字が増えるだけで、
 * 要素・属性・scheme が増えることは無い。
 */
const PROFILES = ['github', 'comment'] as const;

function parse(html: string): Document {
  return new DOMParser().parseFromString(html, 'text/html');
}

describe.each(PROFILES)('生の HTML は字として出る（%s profile）', (profile) => {
  const render = (source: string) => renderDescription(source, { scope: 'raw', profile });

  it.each([
    ['ジェネリクス', 'Vec<String>', '<p>Vec&lt;String&gt;</p>'],
    ['語の中の山括弧', 'a<b>c', '<p>a&lt;b&gt;c</p>'],
    ['不等号を二つ含む式', '1 < 2 > 0', '<p>1 &lt; 2 &gt; 0</p>'],
    ['行中の太字 tag', '<b>太字</b>', '<p>&lt;b&gt;太字&lt;/b&gt;</p>'],
    ['行中の tag と前後の字', '前 <b>太字</b> 後', '<p>前 &lt;b&gt;太字&lt;/b&gt; 後</p>'],
    ['HTML のコメント', '<!-- コメント -->', '<p>&lt;!-- コメント --&gt;</p>'],
    ['HTML のコメントと同じ行の字', '<!-- 隠す --> 見える', '<p>&lt;!-- 隠す --&gt; 見える</p>'],
    ['script', '<script>alert(1)</script>', '<p>&lt;script&gt;alert(1)&lt;/script&gt;</p>'],
    [
      '行中の script',
      'a <script>alert(1)</script> b',
      '<p>a &lt;script&gt;alert(1)&lt;/script&gt; b</p>',
    ],
  ])('%s: 書いたとおりの字で出る', async (_name, source, expected) => {
    expect(await render(source)).toBe(expected);
    expect(parse(expected).body.textContent).toBe(source);
  });

  it('HTML の行で始まる段落は、丸ごと字で出る', async () => {
    const html = await render('<p>段落</p>');
    expect(html).toBe('<p>&lt;p&gt;段落&lt;/p&gt;</p>');
    expect(parse(html).body.textContent).toBe('<p>段落</p>');
  });

  it('複数行の HTML の行は、改行を保って一つの段落の字として出る', async () => {
    const html = await render('<div>\nline\n</div>');
    expect(parse(html).body.textContent).toBe('<div>\nline\n</div>');
    expect(parse(html).body.querySelectorAll('p')).toHaveLength(1);
  });

  it('HTML の行の後ろの markdown は、普通に描かれる', async () => {
    const html = await render('<div>\n\n**強調**\n\n</div>');
    expect(html).toContain('<strong>強調</strong>');
    expect(html).toContain('&lt;div&gt;');
    expect(html).toContain('&lt;/div&gt;');
  });

  it('見出し・リスト・引用・表・リンクの中の HTML も字で出る', async () => {
    const html = await render(
      [
        '# <h1>t</h1>',
        '',
        '- <li>a</li>',
        '',
        '> <q>quote</q>',
        '',
        '| a |',
        '|---|',
        '| <i>x</i> |',
        '',
        '[<b>l</b>](https://example.com)',
      ].join('\n'),
    );
    const doc = parse(html);
    expect(doc.querySelector('h1')?.textContent).toBe('<h1>t</h1>');
    expect(doc.querySelector('li')?.textContent).toBe('<li>a</li>');
    expect(doc.querySelector('blockquote')?.textContent?.trim()).toBe('<q>quote</q>');
    expect(doc.querySelector('td')?.textContent).toBe('<i>x</i>');
    expect(doc.querySelector('a')?.textContent).toBe('<b>l</b>');
    expect(doc.querySelector('a')?.getAttribute('href')).toBe('https://example.com');
  });

  it('alert の中の HTML も字で出て、alert は alert のまま', async () => {
    const html = await render('> [!NOTE]\n> <b>x</b> y');
    expect(html).toContain('kfm-alert--note');
    expect(html).toContain('&lt;b&gt;x&lt;/b&gt; y');
  });

  it('code span と code block の中は今どおり（二重に escape されぬ）', async () => {
    const html = await render('`<b>code</b>`\n\n```\n<i>x</i>\n```');
    const doc = parse(html);
    expect(doc.querySelector('p code')?.textContent).toBe('<b>code</b>');
    expect(doc.querySelector('kfm-code code')?.textContent).toContain('<i>x</i>');
    expect(html).not.toContain('&amp;lt;');
  });

  it('実体参照は markdown の規則どおり一度だけ解かれ、字として出る', async () => {
    const html = await render('&lt;b&gt; と <b>&amp;</b>');
    expect(parse(html).body.textContent).toBe('<b> と <b>&</b>');
    expect(html).not.toContain('<b>');
  });

  it('メンションの印は生の HTML ではなく、今どおり札になる', async () => {
    const id = '11111111-1111-4111-8111-111111111111';
    const html = await renderDescription(`<@${id}> と <b>`, {
      scope: 'raw-mention',
      profile,
      mentions: { [id]: { status: 'member', name: 'alice' } },
    });
    expect(html).toContain('kfm-mention');
    expect(html).toContain('@alice');
    expect(html).toContain('&lt;b&gt;');
  });

  it('自動 link（<https://…>）は生の HTML ではなく、今どおり link になる', async () => {
    const html = await render('<https://example.com/a>');
    expect(html).toBe('<p><a href="https://example.com/a">https://example.com/a</a></p>');
  });
});

/**
 * 字として出した後に、tag・on 属性・危険な scheme が生えぬこと。
 * 出力を HTML として読み直し（ブラウザが v-html で読むのと同じ）、要素と属性を数える。
 * 出てよい要素は段落（と comment の <br>）だけで、属性は一つも無い。
 */
const ATTACKS: [string, string][] = [
  ['引用符で属性を壊す', '<a href="x" title="\\"><script>alert(1)</script>">t</a>'],
  ['引用符で属性を壊す（単引用符）', "<img src='x' alt='\\'><script>alert(1)</script>'>"],
  ['引用符を閉じて on 属性を足す', '<a href="x" title="a" onmouseover="alert(1)" x="">t</a>'],
  ['入れ子の tag（外側が内側を作る）', '<<script>script>alert(1)</script>'],
  ['入れ子の tag（閉じ tag の中）', '<scr<script>ipt>alert(1)</scr</script>ipt>'],
  ['tag の中の tag', '<b <i onclick="alert(1)">x</i>>'],
  ['実体参照で作る山括弧', '&lt;script&gt;alert(1)&lt;/script&gt;'],
  ['数値参照で作る山括弧', '&#60;script&#62;alert(1)&#60;/script&#62;'],
  ['十六進の参照で作る山括弧', '&#x3c;img src=x onerror=alert(1)&#x3e;'],
  ['参照と生の tag の混在', '&lt;img src=x onerror=alert(1)&gt;<img src=x onerror=alert(2)>'],
  ['参照で作る引用符', '<a href=&quot;x&quot; onclick=&quot;alert(1)&quot;>t</a>'],
  ['javascript: の href', '<a href="javascript:alert(1)">x</a>'],
  ['参照で崩した javascript:', '<a href="jav&#x61;script:alert(1)">x</a>'],
  ['data: の src', '<iframe src="data:text/html,<script>alert(1)</script>"></iframe>'],
  ['svg の onload', '<svg onload=alert(1)>'],
  ['style の注入', '<style>body{display:none}</style><p style="x:y">t</p>'],
  ['HTML のコメントで tag を隠す', '<!--><script>alert(1)</script>-->'],
  ['CDATA 風', '<![CDATA[<script>alert(1)</script>]]>'],
  ['処理命令風', '<?php alert(1) ?>'],
  ['閉じ tag だけ', '</p><script>alert(1)</script><p>'],
];

describe.each(PROFILES)(
  '生の HTML を字にしても、tag・属性・scheme は生えぬ（%s profile）',
  (profile) => {
    it.each(ATTACKS)('%s', async (_name, source) => {
      const html = await renderDescription(source, { scope: 'attack', profile });
      const doc = parse(html);
      const elements = [...doc.body.querySelectorAll('*')];

      // 出てよい要素は段落と（comment の）改行だけ。script・a・img・svg・style などは一つも無い
      expect(elements.map((e) => e.tagName)).toEqual(elements.map(() => 'P'));
      for (const element of elements) expect(element.getAttributeNames()).toEqual([]);
      // 出力の生の文字列にも、tag の開き・on 属性の形・危険な scheme の「要素」は無い
      expect(html).not.toMatch(/<(?!\/?p>)/);
      expect(doc.querySelector('[href],[src],[style],[onclick],[onerror],[onload]')).toBeNull();
      // 字は残る（落ちておらぬ）
      expect((doc.body.textContent ?? '').length).toBeGreaterThan(0);
    });

    it('二度 render した出力（保存し直し）でも、結果が変わらぬ', async () => {
      const source = 'Vec<String> と <b>x</b>';
      const once = await renderDescription(source, { scope: 'twice', profile });
      const again = await renderDescription(source, { scope: 'twice', profile });
      expect(again).toBe(once);
    });
  },
);

describe('remarkKfmRawHtml（mdast の形）', () => {
  const run = (source: string) => {
    const processor = unified().use(remarkParse).use(remarkKfmRawHtml);
    return processor.runSync(processor.parse(source));
  };
  const types = (node: unknown, out: string[] = []): string[] => {
    const n = node as { type: string; children?: unknown[] };
    out.push(n.type);
    n.children?.forEach((c) => types(c, out));
    return out;
  };

  it('陽性対照: 本プラグインを通さぬ parse の結果には html ノードがある', () => {
    expect(types(unified().use(remarkParse).parse('a<b>c\n\n<div>x</div>'))).toContain('html');
  });

  it('html ノードは残らず、行中は text に繋がり、行の HTML は paragraph になる', () => {
    const tree = run('a<b>c\n\n<div>x</div>') as Root;
    expect(types(tree)).not.toContain('html');
    expect(tree.children.map((c) => c.type)).toEqual(['paragraph', 'paragraph']);
    expect((tree.children[0] as Paragraph).children).toEqual([
      expect.objectContaining({ type: 'text', value: 'a<b>c' }),
    ]);
    expect((tree.children[1] as Paragraph).children).toEqual([
      expect.objectContaining({ type: 'text', value: '<div>x</div>' }),
    ]);
  });
});
