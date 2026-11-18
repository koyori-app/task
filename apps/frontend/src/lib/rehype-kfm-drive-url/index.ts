/**
 * rehype-kfm-drive-url — KFM 拡張: 本文に書かれた Drive のファイルの URL
 * `/v1/drive/files/{id}/content` を、ブラウザが引ける `/api/v1/…` に替えて描く (TASK-137)。
 *
 * 本文には API の相対パスを書く。本文は CLI・デスクトップ・MCP も読み、`/api` は web の中継の
 * 都合だからである。web だけが描画時に `/api` を足す (favicon の `faviconProxyPath` と同じ固定の接頭辞)。
 *
 * 規則:
 * - 対象は `img[src]` と `a[href]` のうち、値が Drive の URL に**完全一致**するものだけ。
 *   後ろにパスやクエリが付いたもの、UUID でないもの、絶対 URL は触らない。
 * - `a` は href を替え、`download` を付ける (配信は `Content-Disposition: attachment` 固定で、
 *   Vike は `download` のあるリンクのクリックを画面遷移として拾わない)。
 * - `img` は `<kfm-media data-kind>` で包み、`alt` (ファイル名) の拡張子で描き分ける:
 *   動画は `<video controls>`、音声は `<audio controls>`、それ以外は `<img>` のまま。自動再生はしない。
 *   末尾に hidden のダウンロードのリンクを置き、読み込めなかった時に client の custom element
 *   (element.ts) がメディアと入れ替える。
 * - リンクの中の画像 (`[![a.png](…)](…)`) は src を替えるだけで包まない。包むとリンクの中に
 *   リンクが入り、HTML として成り立たない。
 * - inline style は出さない (FORBID_ATTR: ['style'] と一枚岩)。新しいタグ・属性は schema.ts と同時に増減する。
 * - options は受け取らない。_renderer.ts の processor fingerprint はプラグインの関数名しか
 *   見ないため、closure に閉じた options はキャッシュキーに現れない (rehype-kfm-link-favicon と同じ理由)。
 */
import type { Element, ElementContent, Root } from 'hast';
import { SKIP, visit } from 'unist-util-visit';
import { KFM_MEDIA_TAG } from './_tag';

export { KFM_MEDIA_TAG } from './_tag';
export { kfmDriveUrlSanitizeSchema } from './schema';

export type DriveMediaKind = 'image' | 'video' | 'audio';

const DRIVE_CONTENT_RE =
  /^\/v1\/drive\/files\/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\/content$/i;

/** ブラウザが引くパス。Drive のファイルの URL に完全一致しなければ undefined */
export function driveContentProxyPath(src: string): string | undefined {
  return DRIVE_CONTENT_RE.test(src) ? `/api${src}` : undefined;
}

/**
 * 名前の拡張子で決まる種類。本文の URL には拡張子も MIME も無いので、描画側が種類を知る材料は
 * 名前だけである。画像の 2 つは、MIME が `image/` で届かない環境でも画像として埋め込むために持つ
 * (他の画像は MIME で分かる)。一覧に無ければ undefined。
 */
const MEDIA_KIND_BY_EXTENSION: Readonly<Record<string, DriveMediaKind>> = {
  heic: 'image',
  heif: 'image',
  mp4: 'video',
  mov: 'video',
  webm: 'video',
  m4v: 'video',
  mp3: 'audio',
  wav: 'audio',
  m4a: 'audio',
  ogg: 'audio',
  caf: 'audio',
  aac: 'audio',
};

export function driveMediaKind(name: string): DriveMediaKind | undefined {
  const dot = name.lastIndexOf('.');
  if (dot < 0) return undefined;
  const extension = name.slice(dot + 1).toLowerCase();
  return Object.hasOwn(MEDIA_KIND_BY_EXTENSION, extension)
    ? MEDIA_KIND_BY_EXTENSION[extension]
    : undefined;
}

function driveSrc(node: Element): string | undefined {
  const src = node.properties.src;
  return typeof src === 'string' ? driveContentProxyPath(src) : undefined;
}

/** リンクの中の画像は src を替えるだけにする (上の規則) */
function rewriteImagesInside(node: Element): void {
  for (const child of node.children) {
    if (child.type !== 'element') continue;
    if (child.tagName === 'img') {
      const src = driveSrc(child);
      if (src !== undefined) child.properties.src = src;
    }
    rewriteImagesInside(child);
  }
}

function mediaElement(kind: DriveMediaKind, img: Element, src: string, name: string): Element {
  if (kind === 'video') {
    return {
      type: 'element',
      tagName: 'video',
      properties: { src, controls: true, preload: 'metadata', playsInline: true, title: name },
      children: [],
    };
  }
  if (kind === 'audio') {
    return {
      type: 'element',
      tagName: 'audio',
      properties: { src, controls: true, preload: 'metadata', title: name },
      children: [],
    };
  }
  return { ...img, properties: { ...img.properties, src } };
}

export function rehypeKfmDriveUrl() {
  return function rehypeKfmDriveUrlTransform(tree: Root): void {
    visit(tree, 'element', (node, index, parent) => {
      if (node.tagName === 'a') {
        const href = node.properties.href;
        const proxied = typeof href === 'string' ? driveContentProxyPath(href) : undefined;
        if (proxied !== undefined) {
          node.properties.href = proxied;
          node.properties.download = true;
        }
        rewriteImagesInside(node);
        // 中は上で見た。<a> の入れ子は HTML 上あり得ない
        return SKIP;
      }
      if (node.tagName !== 'img' || parent === undefined || index === undefined) return;
      const src = driveSrc(node);
      if (src === undefined) return;
      const alt = node.properties.alt;
      const name = typeof alt === 'string' ? alt : '';
      const kind = driveMediaKind(name) ?? 'image';
      const wrapper: Element = {
        type: 'element',
        tagName: KFM_MEDIA_TAG,
        properties: { dataKind: kind },
        children: [
          mediaElement(kind, node, src, name),
          {
            type: 'element',
            tagName: 'a',
            properties: { hidden: true, href: src, download: true },
            children: [{ type: 'text', value: name || 'ダウンロード' }],
          },
        ],
      };
      (parent.children as ElementContent[])[index] = wrapper;
      // 包んだ中 (同じ img) をもう一度訪れない
      return [SKIP, index + 1];
    });
  };
}
