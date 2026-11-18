// @vitest-environment happy-dom
// <kfm-media> の client 層（読み込めなかったメディアをダウンロードのリンクへ替える）
import { afterEach, beforeAll, describe, expect, it } from 'vitest';

import { createKfmMediaElement, KFM_MEDIA_TAG } from '../rehype-kfm-drive-url/element';

const SRC = '/api/v1/drive/files/0f8fad5b-d9cb-469f-a165-70867728950e/content';

beforeAll(() => {
  if (customElements.get(KFM_MEDIA_TAG) === undefined) {
    customElements.define(KFM_MEDIA_TAG, createKfmMediaElement());
  }
});

afterEach(() => {
  document.body.innerHTML = '';
});

/** SSR の HTML と同じ形（メディア ＋ hidden のリンク）を、文書につなぐ前の状態で作る */
function build(kind: 'image' | 'video' | 'audio', prepare?: (media: HTMLElement) => void) {
  const host = document.createElement(KFM_MEDIA_TAG);
  host.dataset.kind = kind;
  const media = document.createElement(kind === 'image' ? 'img' : kind);
  media.setAttribute('src', SRC);
  const link = document.createElement('a');
  link.hidden = true;
  link.href = SRC;
  link.download = '';
  link.textContent = 'a.mov';
  host.append(media, link);
  // happy-dom は画像を読み込まず、img は最初から complete（幅 0）になる。実ブラウザの
  // 「読み込み中」に合わせ、既定は complete でない状態にする
  if (prepare) prepare(media);
  else if (kind === 'image') Object.defineProperty(media, 'complete', { value: false });
  return { host, media, link };
}

function expectFailed(parts: ReturnType<typeof build>) {
  expect(parts.media.hidden).toBe(true);
  expect(parts.link.hidden).toBe(false);
  expect(parts.host.dataset.kfmMedia).toBe('failed');
}

function expectUntouched(parts: ReturnType<typeof build>) {
  expect(parts.media.hidden).toBe(false);
  expect(parts.link.hidden).toBe(true);
  expect(parts.host.dataset.kfmMedia).toBeUndefined();
}

describe('<kfm-media>', () => {
  it.each(['image', 'video', 'audio'] as const)(
    '%s の error（バブリングしない）でリンクに入れ替える',
    (kind) => {
      const parts = build(kind);
      document.body.append(parts.host);
      expectUntouched(parts);
      parts.media.dispatchEvent(new Event('error'));
      expectFailed(parts);
    },
  );

  it('読み込めたら何もしない', () => {
    const parts = build('image');
    document.body.append(parts.host);
    parts.media.dispatchEvent(new Event('load'));
    expectUntouched(parts);
  });

  it('接続より前に失敗していた画像（complete で幅 0）も入れ替える', () => {
    const parts = build('image', (media) => {
      Object.defineProperty(media, 'complete', { value: true });
      Object.defineProperty(media, 'naturalWidth', { value: 0 });
    });
    document.body.append(parts.host);
    expectFailed(parts);
  });

  it('読み込み済みの画像（complete で幅あり）・読み込み中の画像は入れ替えない', () => {
    const loaded = build('image', (media) => {
      Object.defineProperty(media, 'complete', { value: true });
      Object.defineProperty(media, 'naturalWidth', { value: 320 });
    });
    const loading = build('image', (media) => {
      Object.defineProperty(media, 'complete', { value: false });
      Object.defineProperty(media, 'naturalWidth', { value: 0 });
    });
    document.body.append(loaded.host, loading.host);
    expectUntouched(loaded);
    expectUntouched(loading);
  });

  it('接続より前に失敗していた動画（error あり）も入れ替える', () => {
    const parts = build('video', (media) => {
      Object.defineProperty(media, 'error', { value: { code: 4 } });
    });
    document.body.append(parts.host);
    expectFailed(parts);
  });

  it('失敗していない動画（error が null）は入れ替えない', () => {
    const parts = build('video', (media) => {
      Object.defineProperty(media, 'error', { value: null });
    });
    document.body.append(parts.host);
    expectUntouched(parts);
  });

  it('メディア以外の子（リンク）の error では入れ替えない', () => {
    const parts = build('image');
    document.body.append(parts.host);
    parts.link.dispatchEvent(new Event('error'));
    expectUntouched(parts);
  });

  it('つなぎ直してもリスナーを重ねず、外している間の error は拾わない', () => {
    const parts = build('video', (media) => {
      Object.defineProperty(media, 'error', { value: null });
    });
    let calls = 0;
    const original = (parts.host as unknown as { fallBack: () => void }).fallBack;
    (parts.host as unknown as { fallBack: () => void }).fallBack = function fallBack() {
      calls += 1;
      original.call(this);
    };
    document.body.append(parts.host);
    parts.host.remove();
    parts.media.dispatchEvent(new Event('error'));
    expect(calls).toBe(0);
    expectUntouched(parts);

    document.body.append(parts.host);
    document.body.append(parts.host);
    parts.media.dispatchEvent(new Event('error'));
    expect(calls).toBe(1);
    expectFailed(parts);
  });
});
