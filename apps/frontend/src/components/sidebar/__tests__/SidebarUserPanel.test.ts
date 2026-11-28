import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';

import {
  type BeforeInstallPromptEvent,
  captureInstallPrompt,
  resetInstallPromptForTest,
} from '@/lib/pwa/install-prompt';
import SidebarUserPanel from '../SidebarUserPanel.vue';

enableAutoUnmount(afterEach);
afterEach(() => resetInstallPromptForTest());

function mountPanel(options: { path?: string; rail?: boolean; onLogout?: () => void } = {}) {
  return mount(SidebarUserPanel, {
    attachTo: document.body,
    props: {
      user: { name: 'Alice', email: 'alice@example.test', avatar: '' },
      rail: options.rail,
      onLogout: options.onLogout,
    },
    global: {
      provide: {
        'vike-vue:usePageContext': {
          urlPathname: options.path ?? '/alpha',
          routeParams: {},
        },
      },
    },
  });
}

type Wrapper = ReturnType<typeof mountPanel>;

function gear(wrapper: Wrapper) {
  return wrapper.find('a[aria-label="アカウント設定"]');
}

function card(wrapper: Wrapper) {
  return wrapper.find('[role="dialog"][aria-label="アカウント"]');
}

/** カードの項目は閉じていても DOM にあるので、名前で引く */
function cardItems(wrapper: Wrapper) {
  return card(wrapper).findAll('[data-user-panel-item]');
}

function cardItem(wrapper: Wrapper, label: string) {
  return cardItems(wrapper).find((item) => item.text() === label);
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

describe('SidebarUserPanel', () => {
  it('points the gear to the profile and marks it under /settings only', () => {
    expect(gear(mountPanel()).attributes('href')).toBe('/settings/profile');

    const inSettings = gear(mountPanel({ path: '/settings/security' }));
    expect(inSettings.classes()).toContain('bg-sidebar-accent');
    expect(inSettings.attributes('aria-current')).toBe('page');

    for (const path of ['/alpha/settings', '/settingsx']) {
      const outside = gear(mountPanel({ path }));
      expect(outside.classes(), path).not.toContain('bg-sidebar-accent');
      expect(outside.attributes('aria-current'), path).toBeUndefined();
    }
  });

  it('opens the card from the user with the profile link and logout, and no notifications', async () => {
    const wrapper = mountPanel();
    expect(card(wrapper).attributes('hidden')).toBeDefined();

    await wrapper.find('button[aria-haspopup="dialog"]').trigger('click');

    expect(card(wrapper).attributes('hidden')).toBeUndefined();
    expect(cardItems(wrapper).map((item) => item.text())).toEqual([
      'プロフィールを編集',
      'ログアウト',
    ]);
    expect(cardItem(wrapper, 'プロフィールを編集')!.attributes('href')).toBe('/settings/profile');
    expect(wrapper.text()).not.toContain('通知');
  });

  it('calls onLogout from the logout item', async () => {
    const onLogout = vi.fn();
    const wrapper = mountPanel({ onLogout });
    await wrapper.find('button[aria-haspopup="dialog"]').trigger('click');

    await cardItem(wrapper, 'ログアウト')!.trigger('click');

    expect(onLogout).toHaveBeenCalledTimes(1);
  });

  it('shows only the avatar button when collapsed, which still opens the card', async () => {
    const wrapper = mountPanel({ rail: true });

    expect(gear(wrapper).exists()).toBe(false);
    const trigger = wrapper.find('button[aria-haspopup="dialog"]');
    expect(trigger.attributes('aria-label')).toBe('Alice');
    await trigger.trigger('click');

    expect(card(wrapper).attributes('hidden')).toBeUndefined();
    expect(cardItem(wrapper, 'プロフィールを編集')).toBeDefined();
  });
});

describe('SidebarUserPanel install item', () => {
  it('is absent when the browser has not offered an install', () => {
    const wrapper = mountPanel();

    expect(cardItem(wrapper, 'アプリをインストール')).toBeUndefined();
  });

  it('appears after beforeinstallprompt and opens the browser prompt on click', async () => {
    const offer = offerInstall();
    const wrapper = mountPanel();

    offer.dispatch();
    await flushPromises();
    const item = cardItem(wrapper, 'アプリをインストール');
    expect(item).toBeDefined();

    await item!.trigger('click');
    await flushPromises();

    expect(offer.prompt).toHaveBeenCalledTimes(1);
    expect(cardItem(wrapper, 'アプリをインストール')).toBeUndefined();
  });

  it('disappears after appinstalled', async () => {
    const offer = offerInstall();
    const wrapper = mountPanel();
    offer.dispatch();
    await flushPromises();

    offer.win.dispatchEvent(new Event('appinstalled'));
    await flushPromises();

    expect(cardItem(wrapper, 'アプリをインストール')).toBeUndefined();
  });
});
