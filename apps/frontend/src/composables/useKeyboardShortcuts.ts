import { onMounted, onUnmounted } from 'vue';

const EDITABLE_SELECTOR =
  'input, textarea, select, [contenteditable]:not([contenteditable="false"])';

/**
 * mount 中の画面（fallback でない呼び出し）の bindings。fallback はここにあるキーを譲る。
 * listener の順は mount の順で決まり、クライアント遷移の後はレイアウトが先になるので、
 * defaultPrevented だけでは画面の割り当てを優先できない
 */
const primaryBindings = new Set<Record<string, () => void>>();

/** 判定だけの純粋関数 */
function shouldIgnoreShortcut(event: KeyboardEvent): boolean {
  if (event.defaultPrevented || event.repeat) return true;
  // ブラウザ・OS・既存の ⌘B / ⌘S と衝突させない
  if (event.ctrlKey || event.metaKey || event.altKey) return true;
  // overlay-dismiss-guard.ts と同じ IME 判定
  if (event.isComposing || event.keyCode === 229) return true;
  if (event.target instanceof Element && event.target.closest(EDITABLE_SELECTOR)) return true;
  // タスク詳細オーバーレイや確認ダイアログの上に重ねない
  return !!document.querySelector('[role="dialog"], dialog[open]');
}

/**
 * 修飾キー無しの単打ショートカット。入力中・IME 変換中・ダイアログ表示中は発火しない。
 *
 * `fallback` はレイアウトなど画面をまたぐ割り当て。mount 中の画面が同じキーを持っていれば
 * （その handler が何もしない場合も）譲る
 */
export function useKeyboardShortcuts(
  bindings: Record<string, () => void>,
  options: { fallback?: boolean } = {},
): void {
  const fallback = options.fallback ?? false;
  function onKeydown(event: KeyboardEvent) {
    // Chrome の自動補完は key 無しの keydown を流す
    if (!event.key || shouldIgnoreShortcut(event)) return;
    const key = event.key.toLowerCase();
    const handler = bindings[key];
    if (!handler) return;
    if (fallback && [...primaryBindings].some((primary) => Object.hasOwn(primary, key))) return;
    event.preventDefault();
    handler();
  }
  // onMounted はクライアントでしか走らないので SSR では登録しない
  onMounted(() => {
    if (!fallback) primaryBindings.add(bindings);
    window.addEventListener('keydown', onKeydown);
  });
  onUnmounted(() => {
    if (!fallback) primaryBindings.delete(bindings);
    window.removeEventListener('keydown', onKeydown);
  });
}
