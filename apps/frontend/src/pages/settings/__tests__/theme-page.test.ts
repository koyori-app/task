import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createSSRApp } from 'vue';
import { renderToString } from 'vue/server-renderer';
import { persistedPinia } from '@/components/settings/__tests__/test-pinia';

const pageContext = { urlPathname: '/settings/theme' };
vi.mock('vike-vue/usePageContext', () => ({ usePageContext: () => pageContext }));

import ThemePage from '../theme/+Page.vue';

/**
 * happy-dom の MutationObserver は private field を持つただの class で、ThemeSwitch が ref に入れると
 * Vue が Proxy で包み、observe が落ちる。ブラウザの物は Vue が包まないので、同じく包まれない名札を付ける。
 */
class UnwrappedMutationObserver extends MutationObserver {
  get [Symbol.toStringTag]() {
    return 'MutationObserver';
  }
}

enableAutoUnmount(afterEach);
beforeEach(() => {
  vi.stubGlobal('MutationObserver', UnwrappedMutationObserver);
  localStorage.clear();
  delete document.documentElement.dataset.theme;
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

async function mountPage() {
  const wrapper = mount(ThemePage, {
    attachTo: document.body,
    global: { plugins: [persistedPinia()] },
  });
  await flushPromises();
  return wrapper;
}

describe('/settings/theme の画面', () => {
  it('アカウント設定の一覧とテーマの節を出し、一覧の「テーマ」が現在の項目になる', async () => {
    const wrapper = await mountPage();

    expect(wrapper.get('h1').text()).toBe('アカウント設定');
    expect(wrapper.get('h2').text()).toBe('テーマ');
    expect(wrapper.text()).toContain('画面の配色を切り替えます。');
    expect(wrapper.text()).toContain('この端末のこのブラウザだけで効く設定です');
    expect(wrapper.get('a[aria-current="page"]').attributes('href')).toBe('/settings/theme');

    const row = wrapper.get('[data-setting="theme"]');
    expect(row.text()).toContain('ダークテーマ');
    expect(row.text()).toContain('オンで暗い配色、オフで明るい配色になります。');
    expect(row.get('[role="switch"]').attributes('aria-label')).toBe('ダークテーマ');
  });

  it('スイッチで html の data-theme を切り替え、選んだ値を localStorage に残す', async () => {
    document.documentElement.dataset.theme = 'light';
    const wrapper = await mountPage();
    const toggle = wrapper.get('[data-setting="theme"] [role="switch"]');
    expect(toggle.attributes('aria-checked')).toBe('false');

    await toggle.trigger('click');
    await flushPromises();
    expect(document.documentElement.dataset.theme).toBe('dark');
    expect(localStorage.getItem('koyori-theme')).toBe('dark');
    expect(toggle.attributes('aria-checked')).toBe('true');

    await toggle.trigger('click');
    await flushPromises();
    expect(document.documentElement.dataset.theme).toBe('light');
    expect(localStorage.getItem('koyori-theme')).toBe('light');
    expect(toggle.attributes('aria-checked')).toBe('false');
  });

  it('SSR の HTML は aria-checked="true" で、明るい配色で hydrate しても mismatch せず、mount の後に false になる', async () => {
    const html = await renderToString(createSSRApp(ThemePage).use(persistedPinia()));
    expect(html).toContain('aria-checked="true"');

    document.documentElement.dataset.theme = 'light';
    const container = document.createElement('div');
    container.innerHTML = html;
    document.body.append(container);
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const app = createSSRApp(ThemePage).use(persistedPinia());
    app.mount(container);
    await flushPromises();

    expect(warn).not.toHaveBeenCalled();
    expect(container.querySelector('[role="switch"]')?.getAttribute('aria-checked')).toBe('false');
    app.unmount();
    container.remove();
  });
});
