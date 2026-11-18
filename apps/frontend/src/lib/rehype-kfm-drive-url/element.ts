/**
 * element.ts — `<kfm-media>` custom element (client 専用・軽量シェル)。
 *
 * 仕事は、中の画像・動画・音声をブラウザが読み込めなかった時に、その場でダウンロードのリンクへ
 * 替えることだけ。表示できるかはブラウザとコーデックで決まり、名前の拡張子では分からない
 * (HEIC は Safari だけ、`.mov` は中身次第)。SSR HTML は「メディア ＋ hidden のリンク」で、
 * JS が動かない環境ではブラウザ標準の壊れた画像・空のプレイヤーのままになる。
 *
 * - constructor は factory で遅延 (_client-registry 契約: HTMLElement 不在の SSR / Node で
 *   モジュール評価が落ちない)。import してよいのは _tag.ts だけ (+client.ts が同期で読む)。
 * - `error` はバブリングしないので、自分に capture で付けて子の失敗を拾う。
 * - 登録より前に失敗していたもの (SSR の HTML を読み終えた後で upgrade される) は、接続時に
 *   `img.complete && naturalWidth === 0`・`media.error !== null` で見る。
 * - 状態は `data-kfm-media="failed"` で示す (client だけが立てる。SSR HTML には出さない)。
 */
import { KFM_MEDIA_TAG } from './_tag';

export { KFM_MEDIA_TAG } from './_tag';

type Media = HTMLImageElement | HTMLMediaElement;

function mediaOf(host: HTMLElement): Media | null {
  return host.querySelector<Media>(':scope > img, :scope > video, :scope > audio');
}

function alreadyFailed(media: Media): boolean {
  if (media.localName === 'img') {
    const img = media as HTMLImageElement;
    return img.complete && img.naturalWidth === 0;
  }
  return (media as HTMLMediaElement).error != null;
}

export function createKfmMediaElement(): CustomElementConstructor {
  return class KfmMediaElement extends HTMLElement {
    // 同じ関数を渡すので、再接続で addEventListener を重ねても 1 つのまま
    readonly onMediaError = (event: Event): void => {
      if (event.target === mediaOf(this)) this.fallBack();
    };

    connectedCallback(): void {
      this.addEventListener('error', this.onMediaError, true);
      const media = mediaOf(this);
      if (media !== null && alreadyFailed(media)) this.fallBack();
    }

    disconnectedCallback(): void {
      this.removeEventListener('error', this.onMediaError, true);
    }

    fallBack(): void {
      const media = mediaOf(this);
      const link = this.querySelector<HTMLAnchorElement>(':scope > a');
      // リンクが無い (想定外の中身) なら、見えるものを消さない
      if (media === null || link === null) return;
      media.hidden = true;
      link.hidden = false;
      this.dataset.kfmMedia = 'failed';
    }
  };
}

// KFM_MEDIA_TAG を経由しない直書きを防ぐための再輸出 (rehype-kfm-code/element.ts と同型)
void KFM_MEDIA_TAG;
