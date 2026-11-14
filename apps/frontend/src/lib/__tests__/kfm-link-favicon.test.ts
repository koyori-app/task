// 外部リンクの先頭の favicon（対象と除外、host の正規化と上限、sanitize 通過後の実出力）
import { describe, expect, it } from 'vitest';
import { renderDescription } from '../markup-renderer';
import { favicon } from './kfm-favicon-html';

const MEMBER_ID = '11111111-1111-4111-8111-111111111111';

const label = (n: number) => 'a'.repeat(n);

describe('rehype-kfm-link-favicon (外部リンクの favicon)', () => {
  it('明示リンクの先頭に img を置く (sanitize を class / src / aria-hidden / loading ごと通る)', async () => {
    expect(await renderDescription('[例](https://example.com/x)')).toBe(
      `<p><a href="https://example.com/x">${favicon('example.com')}例</a></p>`,
    );
  });

  it('GFM の autolink literal と <URL> にも付く', async () => {
    expect(await renderDescription('https://example.org/')).toBe(
      `<p><a href="https://example.org/">${favicon('example.org')}https://example.org/</a></p>`,
    );
    expect(await renderDescription('<https://example.org/>')).toBe(
      `<p><a href="https://example.org/">${favicon('example.org')}https://example.org/</a></p>`,
    );
  });

  it('参照リンクにも付く (href が決まった後の rehype 層で見る)', async () => {
    expect(await renderDescription('[x][r]\n\n[r]: https://example.com')).toBe(
      `<p><a href="https://example.com">${favicon('example.com')}x</a></p>`,
    );
  });

  it('同じ host の 2 つのリンクにそれぞれ付く', async () => {
    const html = await renderDescription(
      '[a](https://example.com/1) と [b](https://example.com/2)',
    );
    expect(html.split('kfm-link-favicon').length - 1).toBe(2);
  });

  it('http でも付き、src は同じ', async () => {
    expect(await renderDescription('[a](http://example.com/)')).toBe(
      `<p><a href="http://example.com/">${favicon('example.com')}a</a></p>`,
    );
  });

  it('host を小文字・punycode に正規化し、ポートと認証情報を捨てる', async () => {
    expect(await renderDescription('[a](HTTPS://Example.COM:8443/a)')).toContain(
      favicon('example.com'),
    );
    expect(await renderDescription('[a](https://日本.jp/)')).toContain(favicon('xn--wgv71a.jp'));
    expect(await renderDescription('[a](https://user@example.com:8443/)')).toContain(
      favicon('example.com'),
    );
  });

  describe('付かないもの', () => {
    it('KEY-N・メンション・脚注の戻りリンク', async () => {
      expect(await renderDescription('TASK-1', { taskRef: { tenant: 'acme' } })).not.toContain(
        'kfm-link-favicon',
      );
      expect(
        await renderDescription(`<@${MEMBER_ID}>`, {
          mentions: { [MEMBER_ID]: { status: 'member', name: 'alice' } },
          mentionTenant: 'acme',
        }),
      ).not.toContain('kfm-link-favicon');
      expect(await renderDescription('脚注[^1]\n\n[^1]: 本文')).not.toContain('kfm-link-favicon');
    });

    it('相対・mailto・プロトコル相対', async () => {
      for (const href of ['/path', 'mailto:x@example.com', '//cdn.example.com/x', '#top']) {
        expect(await renderDescription(`[a](${href})`)).not.toContain('kfm-link-favicon');
      }
    });

    it('ドット無し host・IPv6 リテラル', async () => {
      expect(await renderDescription('https://localhost:3000/')).not.toContain('kfm-link-favicon');
      expect(await renderDescription('[a](https://[::1]/)')).not.toContain('kfm-link-favicon');
    });

    it('画像だけのリンク (バッジ) と空のリンク', async () => {
      expect(
        await renderDescription('[![badge](https://img.example.com/b.png)](https://example.com)'),
      ).not.toContain('kfm-link-favicon');
      expect(await renderDescription('[](https://example.com)')).not.toContain('kfm-link-favicon');
    });
  });

  it('host の長さの境界: ラベル 63 / 全長 253 は付き、64 / 254 は付かない', async () => {
    const withLabel = (n: number) => `[a](https://${label(n)}.com/)`;
    expect(await renderDescription(withLabel(63))).toContain('kfm-link-favicon');
    expect(await renderDescription(withLabel(64))).not.toContain('kfm-link-favicon');

    // 全長: 63 + '.' + 63 + '.' + 63 + '.' + 59 = 253 (最後のラベルで調整)
    const hostOf = (total: number) => {
      const parts = [label(63), label(63), label(63)];
      const last = total - parts.join('.').length - 1;
      return `${parts.join('.')}.${label(last)}`;
    };
    expect(hostOf(253)).toHaveLength(253);
    expect(await renderDescription(`[a](https://${hostOf(253)}/)`)).toContain('kfm-link-favicon');
    expect(await renderDescription(`[a](https://${hostOf(254)}/)`)).not.toContain(
      'kfm-link-favicon',
    );
  });
});
