/**
 * rehype-kfm-link-favicon が emit する class の許可宣言。composition root が
 * createRenderer へ渡して sanitize registry と単一ソース化する (_schemas.ts 参照)。
 * img の src / alt / loading / aria-hidden は DOMPurify の既定で通るため、宣言するのは class のみ。
 */
import type { SanitizeSchema } from '../markup-renderer/_sanitize';
import { KFM_LINK_FAVICON_CLASS } from './_class';

export const kfmLinkFaviconSanitizeSchema = {
  classTokens: [KFM_LINK_FAVICON_CLASS],
} as const satisfies SanitizeSchema;
