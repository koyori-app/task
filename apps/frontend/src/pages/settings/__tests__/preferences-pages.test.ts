import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { persistedPinia, seedSettings } from '@/components/settings/__tests__/test-pinia';

const pageContext = { urlPathname: '/settings/preferences' };
vi.mock('vike-vue/usePageContext', () => ({ usePageContext: () => pageContext }));

import ExperimentsPage from '../experiments/+Page.vue';
import PreferencesPage from '../preferences/+Page.vue';

enableAutoUnmount(afterEach);
beforeEach(() => localStorage.clear());

async function mountPage(page: typeof PreferencesPage, path: string) {
  pageContext.urlPathname = path;
  const wrapper = mount(page, { global: { plugins: [persistedPinia()] } });
  await flushPromises();
  return wrapper;
}

describe('/settings/preferences の画面', () => {
  it('アカウント設定の一覧と環境設定の節を出し、一覧の「環境設定」が現在の項目になる', async () => {
    const wrapper = await mountPage(PreferencesPage, '/settings/preferences');

    expect(wrapper.get('h1').text()).toBe('アカウント設定');
    expect(wrapper.get('h2').text()).toBe('環境設定');
    expect(wrapper.get('[data-preferences-section]').text()).toContain('実験的機能を表示する');
    expect(wrapper.get('a[aria-current="page"]').attributes('href')).toBe('/settings/preferences');
  });

  it('この端末のこのブラウザだけで効くことを、画面の上の方に書く', async () => {
    const wrapper = await mountPage(PreferencesPage, '/settings/preferences');

    expect(wrapper.text()).toContain('この端末のこのブラウザだけで効く設定です');
  });

  it('表示を有効にすると、同じ画面の一覧に「実験的機能」が出る', async () => {
    const wrapper = await mountPage(PreferencesPage, '/settings/preferences');
    expect(wrapper.find('a[href="/settings/experiments"]').exists()).toBe(false);

    await wrapper.get('[data-setting="showExperiments"] [role="switch"]').trigger('click');

    expect(wrapper.find('a[href="/settings/experiments"]').exists()).toBe(true);
  });
});

describe('/settings/experiments の画面', () => {
  it('表示が有効なら、フラグの一覧を出し、一覧の「実験的機能」が現在の項目になる', async () => {
    seedSettings({ preferences: { showExperiments: true } });
    const wrapper = await mountPage(ExperimentsPage, '/settings/experiments');

    expect(wrapper.get('h2').text()).toBe('実験的機能');
    expect(wrapper.find('[data-setting="koyoriArcIntegration"]').exists()).toBe(true);
    expect(wrapper.get('a[aria-current="page"]').attributes('href')).toBe('/settings/experiments');
    expect(wrapper.text()).toContain(
      'この端末のこのブラウザだけで有効です。不安定な機能を含みます。',
    );
  });

  it('表示が無効のまま直に開くと、案内を出し、一覧に「実験的機能」は無い', async () => {
    const wrapper = await mountPage(ExperimentsPage, '/settings/experiments');

    expect(wrapper.find('[data-experiments-disabled]').exists()).toBe(true);
    expect(wrapper.find('[data-setting]').exists()).toBe(false);
    expect(wrapper.find('nav a[href="/settings/experiments"]').exists()).toBe(false);
  });
});
