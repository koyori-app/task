import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount, type VueWrapper } from '@vue/test-utils';

import {
  type BeforeInstallPromptEvent,
  captureInstallPrompt,
  resetInstallPromptForTest,
} from '@/lib/pwa/install-prompt';
import NavUser from '../NavUser.vue';

vi.mock('vike/client/router', () => ({ navigate: vi.fn() }));

enableAutoUnmount(afterEach);
afterEach(() => resetInstallPromptForTest());

function mountNavUser() {
  return mount(NavUser, {
    props: { user: { name: 'Alice', email: 'alice@example.test', avatar: '' } },
  });
}

/** メニューの項目は閉じていても DOM にあるので、名前で引く */
function installItem(wrapper: VueWrapper) {
  return wrapper.findAll('[role="menuitem"]').find((item) => item.text() === 'Install app');
}

function offerInstall() {
  const win = new EventTarget() as unknown as Window;
  Object.assign(win, { matchMedia: () => ({ matches: false }), navigator: {} });
  captureInstallPrompt(win);
  const prompt = vi.fn(() => Promise.resolve());
  const event = Object.assign(new Event('beforeinstallprompt'), {
    prompt,
    userChoice: Promise.resolve({ outcome: 'accepted', platform: 'web' }),
  }) as BeforeInstallPromptEvent;
  return { dispatch: () => win.dispatchEvent(event), prompt, win };
}

describe('NavUser install item', () => {
  it('is absent when the browser has not offered an install', () => {
    const wrapper = mountNavUser();

    expect(installItem(wrapper)).toBeUndefined();
  });

  it('appears after beforeinstallprompt and opens the browser prompt on click', async () => {
    const offer = offerInstall();
    const wrapper = mountNavUser();

    offer.dispatch();
    await flushPromises();
    const item = installItem(wrapper);
    expect(item).toBeDefined();

    await item!.trigger('click');
    await flushPromises();

    expect(offer.prompt).toHaveBeenCalledTimes(1);
    expect(installItem(wrapper)).toBeUndefined();
  });

  it('disappears after appinstalled', async () => {
    const offer = offerInstall();
    const wrapper = mountNavUser();
    offer.dispatch();
    await flushPromises();

    offer.win.dispatchEvent(new Event('appinstalled'));
    await flushPromises();

    expect(installItem(wrapper)).toBeUndefined();
  });
});
