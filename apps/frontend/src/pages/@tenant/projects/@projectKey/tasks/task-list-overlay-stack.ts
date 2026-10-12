/**
 * オーバーレイで子タスクを開いたとき、Esc・外側クリックで元のタスクへ戻るための積み。
 *
 * メモリだけに持つ（URL の ?selected= は replaceState で履歴を汚さない契約のまま）。
 */

/** 子を開く: 今のタスクを積む。積んである親へ戻る操作（「親タスク」ボタン）なら積まずに降ろす */
export function pushOverlayParent(
  stack: readonly string[],
  current: string | null,
  next: string,
): string[] {
  if (stack.at(-1) === next) return stack.slice(0, -1);
  if (!current || current === next) return [...stack];
  return [...stack, current];
}

/** 閉じる: 親があればそれへ、無ければ null（全部閉じる） */
export function popOverlayParent(stack: readonly string[]): {
  stack: string[];
  next: string | null;
} {
  return { stack: stack.slice(0, -1), next: stack.at(-1) ?? null };
}
