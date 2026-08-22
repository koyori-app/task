/*
 * 本番 renderer が注入する sanitize スキーマ一覧の単一ソース。
 * composition root (index.ts) の一部だが、index.ts はモジュール副作用で
 * createRenderer が走るため、スキーマ一覧だけ要る消費側 (story の攻撃 probe 等) が
 * 副作用なしで import できるようここへ分離する。コア実装 (_sanitize) はこれを
 * import しない (プラグイン注入の向きは composition root → コアのまま)。
 */
import { gfmSanitizeSchema } from '@/lib/remark-gfm';
import { koyoriAlertsSanitizeSchema } from '@/lib/remark-koyori-alerts';
import type { SanitizeSchema } from './_sanitize';

/** 本番と story probe が共用する sanitize スキーマ一覧 (二重管理禁止) */
export const kfmSanitizeSchemas: readonly SanitizeSchema[] = [
  gfmSanitizeSchema,
  koyoriAlertsSanitizeSchema,
];
