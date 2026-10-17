import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  type BeforeInstallPromptEvent,
  captureInstallPrompt,
  resetInstallPromptForTest,
  useInstallPrompt,
} from '../install-prompt';

afterEach(() => resetInstallPromptForTest());

function fakeWindow({ displayStandalone = false, iosStandalone = false } = {}) {
  const target = new EventTarget();
  return Object.assign(target, {
    matchMedia: (query: string) => ({
      matches: displayStandalone && query === '(display-mode: standalone)',
    }),
    navigator: iosStandalone ? { standalone: true } : {},
  }) as unknown as Window;
}

function beforeInstallPrompt(outcome: 'accepted' | 'dismissed' = 'accepted') {
  const event = new Event('beforeinstallprompt', { cancelable: true });
  const prompt = vi.fn(() => Promise.resolve());
  Object.assign(event, {
    prompt,
    userChoice: Promise.resolve({ outcome, platform: 'web' }),
  });
  return { event: event as BeforeInstallPromptEvent, prompt };
}

describe('install prompt', () => {
  it('stays hidden until the browser offers an install', () => {
    captureInstallPrompt(fakeWindow());

    expect(useInstallPrompt().canInstall.value).toBe(false);
  });

  it('shows once beforeinstallprompt arrives, and keeps the browser default prompt', () => {
    const win = fakeWindow();
    captureInstallPrompt(win);
    const { event } = beforeInstallPrompt();

    win.dispatchEvent(event);

    expect(useInstallPrompt().canInstall.value).toBe(true);
    // preventDefault() would suppress Chrome's own mini-infobar on mobile.
    expect(event.defaultPrevented).toBe(false);
  });

  it('prompts from the button once, then hides (the event is single-use)', async () => {
    const win = fakeWindow();
    captureInstallPrompt(win);
    const { event, prompt } = beforeInstallPrompt('dismissed');
    win.dispatchEvent(event);
    const { canInstall, promptInstall } = useInstallPrompt();

    await expect(promptInstall()).resolves.toBe('dismissed');
    await expect(promptInstall()).resolves.toBeUndefined();

    expect(prompt).toHaveBeenCalledTimes(1);
    expect(canInstall.value).toBe(false);
  });

  it('hides after appinstalled', () => {
    const win = fakeWindow();
    captureInstallPrompt(win);
    win.dispatchEvent(beforeInstallPrompt().event);

    win.dispatchEvent(new Event('appinstalled'));

    expect(useInstallPrompt().canInstall.value).toBe(false);
  });

  it('stays hidden in a standalone window even if an offer arrives', () => {
    const win = fakeWindow({ displayStandalone: true });
    captureInstallPrompt(win);

    win.dispatchEvent(beforeInstallPrompt().event);

    expect(useInstallPrompt().canInstall.value).toBe(false);
  });

  it('stays hidden on iOS Safari (no beforeinstallprompt), installed or not', () => {
    captureInstallPrompt(fakeWindow());
    expect(useInstallPrompt().canInstall.value).toBe(false);

    resetInstallPromptForTest();
    captureInstallPrompt(fakeWindow({ iosStandalone: true }));
    expect(useInstallPrompt().canInstall.value).toBe(false);
  });

  it('does nothing on the server (no window)', () => {
    captureInstallPrompt(undefined);

    expect(useInstallPrompt().canInstall.value).toBe(false);
  });
});
