/**
 * rehype-kfm-link-favicon — KFM 拡張: http / https の絶対 URL を持つリンクの先頭に、
 * リンク先サイトの favicon を `<img>` で足す (TASK-362)。
 *
 * 規則:
 * - 対象は hast の `<a href>` のうち http / https の絶対 URL で、host が有効なもの。
 *   明示リンク・GFM の autolink・参照リンク (`[x][r]`) を同じ場所で扱うため、href が
 *   確定した後の rehype 層で見る (参照リンクは mdast では linkReference で、href は
 *   remark-rehype が definition を引いて初めて決まる)。
 * - 対象外: 相対 href (KEY-N・メンション・アプリ内パス)・`#…` (脚注の戻りリンク)・
 *   `mailto:` 等・プロトコル相対 (`//…`)・ドット無しの host (`localhost`)・IP リテラル
 *   (IPv6)・中身が無いリンク・画像を含むリンク (バッジ)。自アプリ宛ての絶対 URL は
 *   外部と同じ扱い (除外にはアプリのオリジンを renderer へ渡す必要があるため)。
 * - 画像の取得元は backend のプロキシ `GET /v1/favicons/{host}`。ブラウザは
 *   `/api/v1/favicons/{host}` だけを読み、閲覧者の IP も閲覧内容も外部へ出ない。
 *   取れない時は backend が 200 で汎用アイコンを返すので、client に失敗時の処理は要らない。
 * - inline style は出さない (FORBID_ATTR: ['style'] と一枚岩)。見た目は style.css。
 * - options は受け取らない。_renderer.ts の processor fingerprint はプラグインの関数名しか
 *   見ないため、closure に閉じた options はキャッシュキーに現れず他構成の HTML を返し得る
 *   (rehype-kfm-code と同じ理由)。rehype 層へ足すだけで旧 HTML は自動で失効する。
 */
import type { Element, Root } from 'hast';
import { SKIP, visit } from 'unist-util-visit';

export { kfmLinkFaviconSanitizeSchema } from './schema';
export { KFM_LINK_FAVICON_CLASS } from './_class';
import { KFM_LINK_FAVICON_CLASS } from './_class';

/**
 * backend の `parse_host` (service::favicon) と同じ規則。ドット必須、ラベル 63 文字以下、
 * 全長 253 文字以下、英小文字・数字・ハイフン (先頭と末尾はハイフン不可)。
 */
const HOST_RE = /^(?=.{1,253}$)[a-z0-9](?:[a-z0-9-]{0,62})(?:\.[a-z0-9](?:[a-z0-9-]{0,62}))+$/;

export function faviconProxyPath(host: string): string {
  return `/api/v1/favicons/${host}`;
}

/** favicon の対象になる絶対 URL の host (小文字・punycode 済み)。対象外なら undefined */
export function externalLinkHost(href: string): string | undefined {
  let url: URL;
  try {
    url = new URL(href);
  } catch {
    return undefined;
  }
  if (url.protocol !== 'http:' && url.protocol !== 'https:') return undefined;
  const host = url.hostname;
  return HOST_RE.test(host) ? host : undefined;
}

function containsImage(node: Element): boolean {
  return node.children.some(
    (child) => child.type === 'element' && (child.tagName === 'img' || containsImage(child)),
  );
}

export function rehypeKfmLinkFavicon() {
  return function rehypeKfmLinkFaviconTransform(tree: Root): void {
    visit(tree, 'element', (node) => {
      if (node.tagName !== 'a') return;
      const href = node.properties.href;
      if (typeof href !== 'string') return;
      const host = externalLinkHost(href);
      if (host === undefined || node.children.length === 0 || containsImage(node)) return SKIP;
      node.children.unshift({
        type: 'element',
        tagName: 'img',
        properties: {
          className: [KFM_LINK_FAVICON_CLASS],
          src: faviconProxyPath(host),
          alt: '',
          ariaHidden: 'true',
          loading: 'lazy',
        },
        children: [],
      });
      // <a> の入れ子は HTML 上あり得ない。内側は見ない
      return SKIP;
    });
  };
}
