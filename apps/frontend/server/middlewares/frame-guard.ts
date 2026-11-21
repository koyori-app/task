import { enhance, MiddlewareOrder, type UniversalMiddleware } from '@universal-middleware/core';

/** MCP の OAuth の同意画面。ほかのサイトの枠の中では表示させない。 */
const GUARDED_PATH = '/oauth/authorize';

/**
 * 同意画面の文書の応答に、枠の中での表示を止めるヘッダーを付ける。
 *
 * 本番のセッション Cookie は SameSite=None なので、ほかのサイトが同意画面を枠に入れて
 * 承認ボタンを押させる攻撃（クリックジャッキング）が成り立つ。ヘッダーが掛かるのは文書の
 * 応答だけで、クライアント側の遷移で開いた場合はページ内の検査が受け持つ。
 *
 * パスは、末尾の `/` と大文字小文字の違いを同じとみなす（`isAuthPagePath` と
 * `prehydrationQueryGuard` の比べ方の両方に合わせる）。
 */
export const frameGuard: UniversalMiddleware = enhance(
  (request) => {
    const pathname = new URL(request.url).pathname.replace(/\/+$/, '').toLowerCase();
    if (pathname !== GUARDED_PATH) {
      return;
    }
    // 後段のハンドラー（ページの描画）が作った応答に掛かる
    return (response: Response) => {
      response.headers.set('Content-Security-Policy', "frame-ancestors 'none'");
      response.headers.set('X-Frame-Options', 'DENY');
      return response;
    };
  },
  {
    name: 'app:frame-guard',
    order: MiddlewareOrder.HEADER_MANAGEMENT,
    // 応答のヘッダーを書き換える
    immutable: false,
  },
);
