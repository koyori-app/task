import { defineComponent, h } from 'vue';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';

import {
  type BeforeInstallPromptEvent,
  captureInstallPrompt,
  resetInstallPromptForTest,
} from '@/lib/pwa/install-prompt';
import NavUser from '../NavUser.vue';

enableAutoUnmount(afterEach);
afterEach(() => resetInstallPromptForTest());

// The real dropdown renders its content in a portal only while open; for these
// tests every part just renders its slot (items keep their attrs and listeners).
const passthrough = (tag: string) =>
  defineComponent({
    inheritAttrs: false,
    setup(_, { slots, attrs }) {
      return () => h(tag, attrs, slots.default?.());
    },
  });

const stubs = {
  DropdownMenu: passthrough('div'),
  DropdownMenuTrigger: passthrough('div'),
  DropdownMenuContent: passthrough('div'),
  DropdownMenuLabel: passthrough('div'),
  DropdownMenuGroup: passthrough('div'),
  DropdownMenuSeparator: passthrough('hr'),
  DropdownMenuItem: passthrough('div'),
};

function mountNavUser() {
  return mount(NavUser, {
    props: { user: { name: 'Alice', email: 'alice@example.test', avatar: '' } },
    global: { stubs },
  });
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

    expect(wrapper.find('[data-testid="install-app"]').exists()).toBe(false);
  });

  it('appears after beforeinstallprompt and opens the browser prompt on click', async () => {
    const offer = offerInstall();
    const wrapper = mountNavUser();

    offer.dispatch();
    await flushPromises();
    const item = wrapper.find('[data-testid="install-app"]');
    expect(item.exists()).toBe(true);

    await item.trigger('click');
    await flushPromises();

    expect(offer.prompt).toHaveBeenCalledTimes(1);
    expect(wrapper.find('[data-testid="install-app"]').exists()).toBe(false);
  });

  it('disappears after appinstalled', async () => {
    const offer = offerInstall();
    const wrapper = mountNavUser();
    offer.dispatch();
    await flushPromises();

    offer.win.dispatchEvent(new Event('appinstalled'));
    await flushPromises();

    expect(wrapper.find('[data-testid="install-app"]').exists()).toBe(false);
  });
});
