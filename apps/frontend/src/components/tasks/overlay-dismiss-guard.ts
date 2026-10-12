/**
 * タスク詳細オーバーレイ（Dialog）を Esc・外側クリックで閉じてよいかの判定。
 *
 * 閉じると器ごと中身が消えるので、入力途中の下書き（コメントなど）が失われる。
 * 入力欄の Esc を自前で処理する箇所（インライン編集など）は既に preventDefault して
 * いるので、ここではそれ以外の入力欄と IME 変換中・入れ子の `<dialog>` を守る。
 */

const EDITABLE_SELECTOR = 'input,textarea,[contenteditable="true"],[contenteditable=""]';

/** 入力途中か（空の input/textarea は入力途中と見なさない） */
function isDirtyEditable(element: Element | null): boolean {
  const editable = element?.closest?.(EDITABLE_SELECTOR);
  if (!editable) return false;
  if (editable instanceof HTMLInputElement || editable instanceof HTMLTextAreaElement) {
    return editable.value.length > 0;
  }
  // contenteditable（CodeMirror）は中身を安全に読めないので常に守る
  return true;
}

/** Esc で器を閉じずに残すか。IME 変換中・入力途中・入れ子の <dialog> が開いている間は残す */
export function shouldKeepOverlayOnEscape(event: KeyboardEvent): boolean {
  // Firefox / Safari は変換確定の Esc でも keydown を流してくる
  if (event.isComposing || event.keyCode === 229) return true;
  const target = event.target instanceof Element ? event.target : null;
  // 削除確認のネイティブ <dialog showModal> は Esc を自分で閉じるが、keydown は window まで届く
  if (target?.closest('dialog[open]')) return true;
  return isDirtyEditable(target);
}

/** 外側クリックで器を閉じずに残すか。pointerdown 時点ではまだ入力欄にフォーカスがある */
export function shouldKeepOverlayOnPointerDownOutside(activeElement: Element | null): boolean {
  return isDirtyEditable(activeElement);
}
