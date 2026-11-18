/**
 * kfm-media のタグ名 (単一ソース)。
 * emit (rehype プラグイン)・sanitize (SanitizeSchema.tags)・client 登録 (_client-registry)
 * の三点が同じタグを参照する。element.ts (client 専用・軽量) と index.ts (rehype 層) の
 * 双方から import されるため、どちらの依存も持たない独立ファイルに置く (rehype-kfm-code/_tag.ts と同型)。
 */
export const KFM_MEDIA_TAG = 'kfm-media' as const;
