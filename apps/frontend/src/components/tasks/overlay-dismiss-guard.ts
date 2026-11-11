/**
 * タスク詳細オーバーレイ（Dialog）を Esc・外側クリックで閉じてよいかの判定。
 *
 * 閉じると器ごと中身が消えるので、入力途中の下書き（コメントなど）が失われる。
 * 入力欄の Esc を自前で処理する箇所（インライン編集など）は既に preventDefault して
 * いるので、ここではそれ以外の入力欄と IME 変換中を守る。
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

/**
 * Esc で器を閉じずに残すか（true なら keydown の既定動作を止める）。IME 変換中・入力途中は残す。
 * 器も <dialog showModal> なので、器の中に重ねた <dialog>（削除確認など）の Esc は手前の dialog の
 * cancel になり、器には届かない。ここで止めると手前の dialog が閉じなくなるので、判定しない
 */
export function shouldKeepOverlayOnEscape(event: KeyboardEvent): boolean {
  // Firefox / Safari は変換確定の Esc でも keydown を流してくる
  if (event.isComposing || event.keyCode === 229) return true;
  const target = event.target instanceof Element ? event.target : null;
  if (target?.closest('dialog[open]')?.parentElement?.closest('dialog[open]')) return false;
  return isDirtyEditable(target);
}

/** 外側クリックで器を閉じずに残すか。pointerdown 時点ではまだ入力欄にフォーカスがある */
export function shouldKeepOverlayOnPointerDownOutside(activeElement: Element | null): boolean {
  return isDirtyEditable(activeElement);
}
