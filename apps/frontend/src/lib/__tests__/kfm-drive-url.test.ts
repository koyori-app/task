// 本文の Drive のファイルの URL（/api の付け足し、動画・音声の描き分け、kfm-media の包み、対象外）
import { describe, expect, it } from 'vitest';
import { renderDescription } from '../markup-renderer';
import { driveContentProxyPath, driveMediaKind } from '../rehype-kfm-drive-url';

const ID = '0f8fad5b-d9cb-469f-a165-70867728950e';
const URL = `/v1/drive/files/${ID}/content`;
const PROXIED = `/api${URL}`;

const fallback = (name: string) => `<a hidden="" href="${PROXIED}" download="">${name}</a>`;

describe.each([
  ['既定', undefined],
  ['comment', 'comment'],
] as const)('rehype-kfm-drive-url (%s profile)', (_label, profile) => {
  const render = (text: string) => renderDescription(text, profile ? { profile } : undefined);

  it('画像は src に /api を足し、kfm-media で包んで hidden のリンクを添える', async () => {
    expect(await render(`![shot.png](${URL})`)).toBe(
      `<p><kfm-media data-kind="image"><img src="${PROXIED}" alt="shot.png">${fallback('shot.png')}</kfm-media></p>`,
    );
  });

  it('リンクは href に /api を足し、download を付ける', async () => {
    expect(await render(`[a.pdf](${URL})`)).toBe(
      `<p><a href="${PROXIED}" download="">a.pdf</a></p>`,
    );
  });

  it('名前の拡張子が動画なら video にする（自動再生しない）', async () => {
    expect(await render(`![clip.mp4](${URL})`)).toBe(
      `<p><kfm-media data-kind="video"><video src="${PROXIED}" controls="" preload="metadata" playsinline="" title="clip.mp4"></video>${fallback('clip.mp4')}</kfm-media></p>`,
    );
  });

  it('名前の拡張子が音声なら audio にする', async () => {
    expect(await render(`![memo.mp3](${URL})`)).toBe(
      `<p><kfm-media data-kind="audio"><audio src="${PROXIED}" controls="" preload="metadata" title="memo.mp3"></audio>${fallback('memo.mp3')}</kfm-media></p>`,
    );
  });

  it('拡張子の大文字小文字は区別しない', async () => {
    expect(await render(`![IMG_0001.MOV](${URL})`)).toContain('<video ');
    expect(await render(`![REC.M4A](${URL})`)).toContain('<audio ');
  });

  it('HEIC は画像として包む（表示できないブラウザではリンクに替わる）', async () => {
    expect(await render(`![IMG_0002.HEIC](${URL})`)).toBe(
      `<p><kfm-media data-kind="image"><img src="${PROXIED}" alt="IMG_0002.HEIC">${fallback('IMG_0002.HEIC')}</kfm-media></p>`,
    );
  });

  it('拡張子の無い名前・一覧に無い拡張子は img のまま', async () => {
    expect(await render(`![図](${URL})`)).toContain(`<img src="${PROXIED}" alt="図">`);
    expect(await render(`![a.mkv](${URL})`)).toContain(`<img src="${PROXIED}" alt="a.mkv">`);
  });

  it('alt が空でもリンクに字を出す', async () => {
    expect(await render(`![](${URL})`)).toContain(
      `<a hidden="" href="${PROXIED}" download="">ダウンロード</a>`,
    );
  });

  it('リンクの中の画像は src を替えるだけで包まない（リンクの入れ子にしない）', async () => {
    expect(await render(`[![a.mp4](${URL})](${URL})`)).toBe(
      `<p><a href="${PROXIED}" download=""><img src="${PROXIED}" alt="a.mp4"></a></p>`,
    );
  });

  it('1 つの段落の 2 つの画像をそれぞれ包む', async () => {
    const html = await render(`![a.png](${URL}) ![b.mp4](${URL})`);
    expect(html.split('<kfm-media').length - 1).toBe(2);
    expect(html).toContain('data-kind="image"');
    expect(html).toContain('data-kind="video"');
  });

  describe('書き換えないもの', () => {
    it.each([
      ['後ろにパスが続く', `${URL}/x`],
      ['UUID でない', '/v1/drive/files/not-a-uuid/content'],
      ['絶対 URL', `https://example.com${URL}`],
      ['クエリが付く', `${URL}?token=x`],
    ])('%s', async (_name, url) => {
      const image = await render(`![a.mp4](${url})`);
      expect(image).toContain(`<img src="${url}" alt="a.mp4">`);
      expect(image).not.toContain('kfm-media');
      expect(image).not.toContain('<video');
      const link = await render(`[a](${url})`);
      expect(link).toContain(`href="${url}"`);
      expect(link).not.toContain('download');
      expect(link).not.toContain('/api/v1/drive');
    });
  });

  it('本文に直書きした <kfm-media> は不活性になる（画像の記法だけが正規の経路）', async () => {
    const html = await render(`<kfm-media data-kind="video" onclick="pwn()">hi</kfm-media>`);
    const doc = new DOMParser().parseFromString(html, 'text/html');
    expect(html).not.toContain('<kfm-media');
    expect(doc.querySelector('kfm-media')).toBeNull();
    expect(doc.querySelector('[onclick]')).toBeNull();
    expect(doc.body.textContent).toBe(
      '<kfm-media data-kind="video" onclick="pwn()">hi</kfm-media>',
    );
  });
});

describe('driveContentProxyPath', () => {
  it('完全一致だけを /api 付きにする（UUID の大文字も受ける）', () => {
    expect(driveContentProxyPath(URL)).toBe(PROXIED);
    expect(
      driveContentProxyPath(
        URL.toUpperCase()
          .replace('/V1/DRIVE/FILES/', '/v1/drive/files/')
          .replace('/CONTENT', '/content'),
      ),
    ).toBeDefined();
    expect(driveContentProxyPath(`${URL}/`)).toBeUndefined();
    expect(driveContentProxyPath(`/api${URL}`)).toBeUndefined();
    expect(driveContentProxyPath('')).toBeUndefined();
  });
});

describe('driveMediaKind', () => {
  it.each([
    ['a.heic', 'image'],
    ['a.heif', 'image'],
    ['a.mp4', 'video'],
    ['a.mov', 'video'],
    ['a.webm', 'video'],
    ['a.m4v', 'video'],
    ['a.mp3', 'audio'],
    ['a.wav', 'audio'],
    ['a.m4a', 'audio'],
    ['a.ogg', 'audio'],
    ['a.caf', 'audio'],
    ['a.aac', 'audio'],
    ['録音.tar.MP3', 'audio'],
  ] as const)('%s は %s', (name, kind) => {
    expect(driveMediaKind(name)).toBe(kind);
  });

  it.each([
    'a.png',
    'a.mkv',
    'a.avi',
    'a.flac',
    'a.3gp',
    'mp4',
    'a.',
    '',
    'constructor.constructor',
  ])('%s は一覧に無い', (name) => {
    expect(driveMediaKind(name)).toBeUndefined();
  });
});
