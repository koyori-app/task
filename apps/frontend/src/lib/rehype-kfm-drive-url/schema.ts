/**
 * rehype-kfm-drive-url が emit するタグ・属性の許可宣言。composition root が
 * createRenderer へ渡して sanitize registry と単一ソース化する (_schemas.ts 参照)。
 * - 属性は SSR で焼く data-kind のみ。読み込みの失敗 (data-kfm-media) は client が立てる値で、
 *   サーバ生成 HTML に現れてはならないため宣言しない (kfm-code の data-kfm-code-copy と同じ整理)。
 * - 中の img / video / audio / a とその属性 (src / controls / preload / playsinline / title /
 *   href / download / hidden) は DOMPurify の既定で通る。
 */
import type { SanitizeSchema } from '../markup-renderer/_sanitize';
import { KFM_MEDIA_TAG } from './_tag';

export const kfmDriveUrlSanitizeSchema = {
  tags: [KFM_MEDIA_TAG],
  attrs: { [KFM_MEDIA_TAG]: ['data-kind'] },
} as const satisfies SanitizeSchema;
