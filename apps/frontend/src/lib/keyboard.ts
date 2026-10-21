/**
 * IME の変換中（確定・取り消しの Enter / Esc を含む）のキー入力か。
 * Safari は compositionend の後に isComposing=false の keydown を流してくるので、keyCode 229 も見る
 */
export function isImeComposing(event: KeyboardEvent): boolean {
  return event.isComposing || event.keyCode === 229;
}
