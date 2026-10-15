/** ドキュメントのタイトルの上限（新規作成・改名で同じ値を使う）。 */
export const MAX_DOCUMENT_TITLE_LENGTH = 120;

/** ファイル名から表示用のタイトルを作る（末尾の `.md` を落とす）。 */
export function documentTitle(fileName: string): string {
  return fileName.replace(/\.md$/i, '');
}

/** タイトルからファイル名を作る。 */
export function documentFileName(title: string): string {
  return `${title.trim()}.md`;
}
