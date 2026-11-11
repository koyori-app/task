/** ドキュメントのタイトルの上限（新規作成・改名で同じ値を使う）。 */
export const MAX_DOCUMENT_TITLE_LENGTH = 120;

/** ファイル名から表示用のタイトルを作る（末尾の `.md` を落とす）。 */
export function documentTitle(fileName: string): string {
  return fileName.replace(/\.md$/i, '');
}

/** この画面で扱うファイルか: MIME が `text/markdown`（パラメータ付きも可）で、名前が `.md` で終わる。 */
export function isDocumentFile(file: { name: string; mime_type: string }): boolean {
  const mime = file.mime_type.split(';')[0]?.trim().toLowerCase();
  return mime === 'text/markdown' && /\.md$/i.test(file.name);
}

/** タイトルからファイル名を作る。 */
export function documentFileName(title: string): string {
  return `${title.trim()}.md`;
}
