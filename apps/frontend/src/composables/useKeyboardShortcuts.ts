import { onMounted, onUnmounted } from 'vue';

const EDITABLE_SELECTOR =
  'input, textarea, select, [contenteditable]:not([contenteditable="false"])';

/** 判定だけの純粋関数（テスト用に export） */
export function shouldIgnoreShortcut(event: KeyboardEvent): boolean {
  if (event.defaultPrevented || event.repeat) return true;
  // ブラウザ・OS・既存の ⌘B / ⌘S と衝突させない
  if (event.ctrlKey || event.metaKey || event.altKey) return true;
  // overlay-dismiss-guard.ts と同じ IME 判定
  if (event.isComposing || event.keyCode === 229) return true;
  if (event.target instanceof Element && event.target.closest(EDITABLE_SELECTOR)) return true;
  // タスク詳細オーバーレイや確認ダイアログの上に重ねない
  return !!document.querySelector('[role="dialog"], dialog[open]');
}

/** 修飾キー無しの単打ショートカット。入力中・IME 変換中・ダイアログ表示中は発火しない */
export function useKeyboardShortcuts(bindings: Record<string, () => void>): void {
  function onKeydown(event: KeyboardEvent) {
    // Chrome の自動補完は key 無しの keydown を流す
    if (!event.key || shouldIgnoreShortcut(event)) return;
    const handler = bindings[event.key.toLowerCase()];
    if (!handler) return;
    event.preventDefault();
    handler();
  }
  // onMounted はクライアントでしか走らないので SSR では登録しない
  onMounted(() => window.addEventListener('keydown', onKeydown));
  onUnmounted(() => window.removeEventListener('keydown', onKeydown));
}
