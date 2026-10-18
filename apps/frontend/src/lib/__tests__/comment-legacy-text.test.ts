import { describe, expect, it } from 'vitest';

import { renderDescription } from '@/lib/markup-renderer';

/**
 * 既存のコメントは「素の文字」として書かれ、保存されておる。本文は移し替えず、そのまま
 * comment profile（GFM ＋ 単独の改行を <br>）で描くので、markdown の記法に当たる並びは見た目が変わる。
 * この表は、どの並びがどう描かれるかを固定する（変わる物を「変わる」と示すことが目的で、
 * 変わる側を良しとするものではない）。
 */
async function render(source: string): Promise<string> {
  const html = await renderDescription(source, {
    scope: 'legacy',
    profile: 'comment',
    mentions: {},
  });
  return html.replace(/\n/g, '');
}

describe('既存の素のコメントを comment profile で描いた時（崩れぬ物）', () => {
  it.each([
    [
      '単独の改行は <br> のまま改行になる',
      '一行目\n二行目\n\n段落',
      '<p>一行目<br>二行目</p><p>段落</p>',
    ],
    ['CRLF の改行も同じ', 'a\r\nb\r\n\r\nc', '<p>a<br>b</p><p>c</p>'],
    ['行末の空白 2 つの改行も同じ', 'a  \nb', '<p>a<br>b</p>'],
    ['行頭の # と数字（#123）は見出しにならぬ', '#123 を見て', '<p>#123 を見て</p>'],
    ['# の直後に空白が無ければ見出しにならぬ', '#未対応の課題', '<p>#未対応の課題</p>'],
    ['語の中の _ は強調にならぬ', 'snake_case_name', '<p>snake_case_name</p>'],
    ['空白で挟んだ * は強調にならぬ', '1 * 2 * 3 = 6', '<p>1 * 2 * 3 = 6</p>'],
    ['素の @username は札にならず字のまま', '@alice さん\n@bob', '<p>@alice さん<br>@bob</p>'],
    ['不等号は escape されて字のまま', 'a < b かつ c > d', '<p>a &lt; b かつ c &gt; d</p>'],
    ['実体参照は字のまま', '&lt;script&gt; と &amp;', '<p>&lt;script&gt; と &amp;</p>'],
    ['バックスラッシュのパスは字のまま', 'C:\\Users\\a\\b', '<p>C:\\Users\\a\\b</p>'],
    ['タスクの参照の形は task の指定が無ければ字のまま', 'ENG-12 を参照', '<p>ENG-12 を参照</p>'],
  ])('%s', async (_name, source, expected) => {
    expect(await render(source)).toBe(expected);
  });
});

describe('既存の素のコメントを comment profile で描いた時（見た目が変わる物）', () => {
  it.each([
    ['行頭 # + 空白は見出しになる', '# 見出し風', '<h1>見出し風</h1>'],
    ['行頭の - は箇条書きになる', '- 手順1\n- 手順2', '<ul><li>手順1</li><li>手順2</li></ul>'],
    ['行頭の * も箇条書きになる', '* 点\n* 点2', '<ul><li>点</li><li>点2</li></ul>'],
    ['行頭の 1. は番号つきの並びになる', '1. 最初\n2. 次', '<ol><li>最初</li><li>次</li></ol>'],
    ['行頭の 1) も番号つきの並びになる', '1) 最初\n2) 次', '<ol><li>最初</li><li>次</li></ol>'],
    [
      '_ や * の囲みは強調になり、記号が消える',
      '_強調_ と *強調* と **太字**',
      '<p><em>強調</em> と <em>強調</em> と <strong>太字</strong></p>',
    ],
    ['語の中の * も強調になる（掛け算の 2*3*4）', '2*3*4', '<p>2<em>3</em>4</p>'],
    ['パスの中の _x_ が強調になる', '~/dir/_x_', '<p>~/dir/<em>x</em></p>'],
    ['行頭の > は引用になる', '> 引用\n返信', '<blockquote><p>引用<br>返信</p></blockquote>'],
    ['~~ も ~ も打消しになる', '~~消す~~ と ~1~', '<p><del>消す</del> と <del>1</del></p>'],
    [
      'URL は自動で link になる',
      'https://example.com/a_b を見て',
      '<p><a href="https://example.com/a_b">https://example.com/a_b</a> を見て</p>',
    ],
    [
      'メールアドレスは mailto の link になる',
      'x@example.com へ',
      '<p><a href="mailto:x@example.com">x@example.com</a> へ</p>',
    ],
    ['行頭が空白 4 つの行はコードブロックになる', '    indented\ncode?', null],
    ['文の次の行の --- は直前の行を見出しにする', 'a\n---\nb', '<h2>a</h2><p>b</p>'],
  ])('%s', async (_name, source, expected) => {
    const html = await render(source);
    if (expected === null) {
      expect(html).toContain('<kfm-code>');
      return;
    }
    expect(html).toBe(expected);
  });
});

/**
 * 消毒で落ちる字。山括弧で囲んだ英数字は HTML の tag と読まれ、消毒（説明文と同じ）で tag ごと落ちる。
 * 既存のコメントに `Vec<String>` のような字が在れば `<String>` が画面から消える（本文の保存は無傷）。
 */
describe('既存の素のコメントを comment profile で描いた時（字が画面から消える物）', () => {
  it.each([
    ['ジェネリクスの <String> が消える', 'Vec<String> を使う', '<p>Vec を使う</p>'],
    [
      '行内の HTML tag は tag だけ落ち、中の字は残る',
      '見て <b>太字</b> です',
      '<p>見て 太字 です</p>',
    ],
    ['<br> は改行にならず落ちる', '一行<br>二行', '<p>一行二行</p>'],
    ['HTML の行で始まる段落は丸ごと消える', '<p>段落</p>\nつづき', ''],
    ['HTML のコメントは消え、同じ行の字も消える', '<!-- 隠す --> 見える', ''],
  ])('%s', async (_name, source, expected) => {
    expect(await render(source)).toBe(expected);
  });
});

/**
 * コメントは誰でも書ける。comment profile は説明文（github profile）と同じ sanitize を通るので、
 * 危険な入力は説明文と同じく落ちる。link の target / rel も説明文と同じ（どちらも付けぬ）。
 */
describe('コメントの XSS の守り（説明文と同じ消毒）', () => {
  const attacks: [string, string][] = [
    ['script', 'a <script>alert(1)</script> b'],
    ['script（行頭）', '<script>alert(1)</script>'],
    ['img の onerror', '<img src=x onerror="alert(1)">'],
    ['on 属性つきの a', '<a href="https://example.com" onclick="alert(1)">x</a>'],
    ['生の javascript: link', '<a href="javascript:alert(1)">x</a>'],
    ['markdown の javascript: link', '[x](javascript:alert(1))'],
    ['大文字小文字・空白を混ぜた javascript:', '[x](JaVaScRiPt:alert(1))'],
    ['data: link', '[x](data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==)'],
    ['vbscript: link', '[x](vbscript:msgbox(1))'],
    ['iframe', '<iframe src="https://example.com"></iframe>'],
    ['svg の onload', '<svg onload="alert(1)"></svg>'],
    ['style 属性', '<p style="background:url(javascript:alert(1))">x</p>'],
    ['form', '<form action="https://evil.example"><input name=p></form>'],
    ['img の javascript:', '![x](javascript:alert(1))'],
  ];

  it.each(attacks)('%s は消毒される', async (_name, source) => {
    const html = await renderDescription(source, {
      scope: 'xss',
      profile: 'comment',
      mentions: {},
    });
    expect(html).not.toMatch(/<script|<iframe|<svg|<form|<style/i);
    expect(html).not.toMatch(/\son[a-z]+\s*=/i);
    expect(html).not.toMatch(/javascript:|vbscript:|data:text/i);
    // script の tag は落ちるが中の字は文字として残りうる（実行はされぬ）ので、字の有無は問わぬ
  });

  it('link の target と rel は説明文（github profile）と同じ出力になる', async () => {
    const source = '[安全](https://example.com/x) と <https://example.com/y>';
    const comment = await renderDescription(source, { scope: 'link', profile: 'comment' });
    const description = await renderDescription(source, { scope: 'link', profile: 'github' });
    expect(comment).toBe(description);
    expect(comment).not.toMatch(/target=|rel=/);
  });

  it('メンションの印を含む入力でも、札の名前に仕込んだ HTML は文字として出る', async () => {
    const id = '11111111-1111-4111-8111-111111111111';
    const html = await renderDescription(`<@${id}>`, {
      scope: 'xss-name',
      profile: 'comment',
      mentions: { [id]: { status: 'member', name: '<img src=x onerror=alert(1)>' } },
    });
    expect(html).not.toMatch(/<img/i);
    expect(html).toContain('&lt;img');
  });
});
