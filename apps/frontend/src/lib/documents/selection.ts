/** 選択範囲から作るタスクのタイトルの上限。 */
export const MAX_SELECTION_TITLE_LENGTH = 120;

/**
 * 本文要素の中で選択している文字列を、タスクのタイトルの初期値にする。
 *
 * 選択が無い・本文の外に掛かっている場合は空文字。改行や連続する空白は 1 つの空白に
 * 畳み、120 文字（コードポイント単位。サロゲートペアを割らない）で切る。
 */
export function selectionTitle(selection: Selection | null, container: Node | null): string {
  if (!selection || !container || selection.rangeCount === 0 || selection.isCollapsed) return '';
  const { anchorNode, focusNode } = selection;
  if (!anchorNode || !focusNode) return '';
  if (!container.contains(anchorNode) || !container.contains(focusNode)) return '';
  const text = selection.toString().replace(/\s+/g, ' ').trim();
  return Array.from(text).slice(0, MAX_SELECTION_TITLE_LENGTH).join('').trim();
}
