// アプリとしての install の案内。
//
// - ブラウザの既定の案内は残す: beforeinstallprompt で preventDefault() を呼ばない。
//   呼ぶと Chrome の mobile の既定の案内（mini-infobar）が出なくなる。desktop の
//   アドレスバーの install の印は、呼んでも呼ばなくても出る。
// - 自前のボタンは、beforeinstallprompt を受け取った時だけ出す。この event を持たぬ
//   環境（iOS と iPadOS の Safari、Firefox など）では出さない。iOS の
//   「ホーム画面に追加」は共有の menu から利用者が行う。
// - install 済み（standalone の窓で開いておる、または appinstalled の後）では出さない。
//
// state は client でだけ書き換わる。SSR では captureInstallPrompt を呼ばないので、
// どの request でも canInstall は false のまま。

import { computed, shallowRef } from 'vue';

/** Chromium 系だけが持つ event。標準の型定義に無いので、使う所だけ書く。 */
export interface BeforeInstallPromptEvent extends Event {
  prompt(): Promise<void>;
  readonly userChoice: Promise<{ outcome: 'accepted' | 'dismissed'; platform: string }>;
}

const deferredPrompt = shallowRef<BeforeInstallPromptEvent | null>(null);
const installed = shallowRef(false);
let captured = false;

/** standalone の窓で開いておるか。iOS の Safari は navigator.standalone で示す。 */
export function isRunningStandalone(win: Window): boolean {
  const iosStandalone = (win.navigator as Navigator & { standalone?: boolean }).standalone;
  return win.matchMedia?.('(display-mode: standalone)').matches === true || iosStandalone === true;
}

/**
 * beforeinstallprompt と appinstalled を受け取り始める。+client.ts から hydration の
 * 前に一度だけ呼ぶ（event は頁の読み込みの早いうちに来ることがある）。
 */
export function captureInstallPrompt(
  win: Window | undefined = typeof window === 'undefined' ? undefined : window,
): void {
  if (!win || captured) return;
  captured = true;
  installed.value = isRunningStandalone(win);
  win.addEventListener('beforeinstallprompt', (event) => {
    deferredPrompt.value = event as BeforeInstallPromptEvent;
  });
  win.addEventListener('appinstalled', () => {
    installed.value = true;
    deferredPrompt.value = null;
  });
}

export function useInstallPrompt() {
  const canInstall = computed(() => !installed.value && deferredPrompt.value !== null);

  /** 自前のボタンから呼ぶ。利用者の操作の中で呼ぶこと（prompt() はそれを求める）。 */
  async function promptInstall(): Promise<'accepted' | 'dismissed' | undefined> {
    const event = deferredPrompt.value;
    if (!event) return undefined;
    // prompt() は一つの event につき一度しか呼べぬので、先に手放す。
    deferredPrompt.value = null;
    await event.prompt();
    const choice = await event.userChoice;
    return choice.outcome;
  }

  return { canInstall, promptInstall };
}

/** 試験のための後始末。 */
export function resetInstallPromptForTest() {
  deferredPrompt.value = null;
  installed.value = false;
  captured = false;
}
