// 変換確定の Enter や変換取り消しの Esc は文字入力の一部で、画面の操作ではない。
// 処理ごとに判定を書くと漏れるので、アプリの keydown へ届く前に window の capture で止める。

/** Safari は確定の keydown で isComposing が false になるので、keyCode 229 も見る */
export function isImeKeydown(event: KeyboardEvent): boolean {
  return event.isComposing || event.keyCode === 229;
}

export function guardImeKeydown(event: KeyboardEvent): void {
  if (!isImeKeydown(event)) return;
  // CodeMirror は変換中の keydown も自分の処理に使うので止めない（MarkdownEditor が外へ渡さない）
  if (event.target instanceof Element && event.target.closest('.cm-editor')) return;
  // 止めても既定動作は残るので、dialog の cancel は preventDefault で起こさせない
  if (event.key === 'Escape') event.preventDefault();
  event.stopPropagation();
}

export function installImeKeydownGuard(): () => void {
  window.addEventListener('keydown', guardImeKeydown, true);
  return () => window.removeEventListener('keydown', guardImeKeydown, true);
}
