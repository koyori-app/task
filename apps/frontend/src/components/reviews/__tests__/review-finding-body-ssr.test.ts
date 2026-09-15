// @vitest-environment node
import { describe, expect, it } from 'vitest';
import { createSSRApp, h } from 'vue';
import { renderToString } from 'vue/server-renderer';
import ReviewFindingBody from '../ReviewFindingBody.vue';

/*
 * この画面は vike-vue の SSR を通る。ReviewFindingBody は KFM 描画を
 * 非同期 (動的 import) で行うため、SSR では素のテキスト表示を出す約束である。
 * markdown-editor-ssr.test.ts と同じ流儀で、node 環境 (DOM 無し) で
 * 描いて落ちぬこと・SSR 出力が素のフォールバックであることを機械照合する。
 * client で組み直す時も初期状態は同じ null (素のテキスト) から始まるため、
 * hydration の食い違いは生じない。
 */
describe('ReviewFindingBody の SSR', () => {
  it('DOM 無しで描画でき、出るのは素のテキストのフォールバック', async () => {
    const html = await renderToString(
      createSSRApp({
        render: () =>
          h(ReviewFindingBody, {
            body: '`code` と **強調** を含む本文',
            findingId: 'f-1',
          }),
      }),
    );

    // 落ちずに描け、本文が素のまま読める
    expect(html).toContain('data-review-finding-body');
    expect(html).toContain('`code` と **強調** を含む本文');
    // SSR では KFM HTML を出さない (非同期描画はサーバで完了しない約束)
    expect(html).not.toContain('<strong>');
    expect(html).not.toContain('<code>');
  });
});
