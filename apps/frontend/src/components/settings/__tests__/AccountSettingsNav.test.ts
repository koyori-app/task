import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createSSRApp, defineComponent, h, nextTick } from 'vue';
import { renderToString } from 'vue/server-renderer';
import AccountSettingsNav from '@/components/settings/AccountSettingsNav.vue';
import { usePreferencesStore } from '@/stores/preferences';
import { persistedPinia, seedSettings } from './test-pinia';

enableAutoUnmount(afterEach);

const App = defineComponent({
  render: () => h(AccountSettingsNav, { currentPath: '/settings/profile' }),
});

beforeEach(() => localStorage.clear());
afterEach(() => vi.restoreAllMocks());

function mountNav(currentPath = '/settings/profile') {
  const pinia = persistedPinia();
  const wrapper = mount(AccountSettingsNav, {
    props: { currentPath },
    global: { plugins: [pinia] },
  });
  return { wrapper, pinia };
}

const linkTitles = (wrapper: ReturnType<typeof mountNav>['wrapper']) =>
  wrapper.findAll('nav > a, nav > span').map((n) => n.text());

describe('AccountSettingsNav の環境設定と実験的機能', () => {
  it('「環境設定」は /settings/preferences への link になる', async () => {
    const { wrapper } = mountNav();
    await flushPromises();

    const link = wrapper.findAll('a').find((a) => a.text() === '環境設定');
    expect(link?.attributes('href')).toBe('/settings/preferences');
  });

  it('表示が無効（既定）なら「実験的機能」の項目も区切り線も無い', async () => {
    const { wrapper } = mountNav();
    await flushPromises();

    expect(wrapper.text()).not.toContain('実験的機能');
    expect(wrapper.find('[data-nav-separator]').exists()).toBe(false);
    expect(wrapper.find('a[href="/settings/experiments"]').exists()).toBe(false);
  });

  it('表示が有効なら、一番下（セッションの後、区切り線の下）に PhFlask の項目が在る', async () => {
    seedSettings({ preferences: { showExperiments: true } });
    const { wrapper } = mountNav();
    await flushPromises();

    const link = wrapper.get('a[href="/settings/experiments"]');
    expect(link.text()).toBe('実験的機能');
    expect(link.find('svg').exists()).toBe(true);
    // 並びの最後で、その直前が区切り線、さらにその前が「セッション」
    const children = [...wrapper.get('nav').element.children];
    expect(children.at(-1)).toBe(link.element);
    expect(children.at(-2)?.getAttribute('data-nav-separator')).toBe('');
    expect(children.at(-3)?.textContent?.trim()).toBe('セッション');
  });

  it('現在の道が /settings/experiments なら aria-current になる', async () => {
    seedSettings({ preferences: { showExperiments: true } });
    const { wrapper } = mountNav('/settings/experiments');
    await flushPromises();

    expect(wrapper.get('a[href="/settings/experiments"]').attributes('aria-current')).toBe('page');
  });

  it('表示を切り替えると、項目の出し入れが画面に追いつく', async () => {
    const { wrapper, pinia } = mountNav();
    await flushPromises();
    const store = usePreferencesStore(pinia);

    store.set('showExperiments', true);
    await nextTick();
    expect(wrapper.find('a[href="/settings/experiments"]').exists()).toBe(true);

    store.set('showExperiments', false);
    await nextTick();
    expect(wrapper.find('a[href="/settings/experiments"]').exists()).toBe(false);
    expect(linkTitles(wrapper)).toContain('セッション');
  });
});

describe('AccountSettingsNav の hydration（SSR と最初の描画が食い違わぬ）', () => {
  it('server は localStorage を読めぬので、表示を有効にしていても項目を出さない', async () => {
    // server 側の store が仮に有効の値を持っていても、mount の前は既定値で描く
    const pinia = persistedPinia();
    usePreferencesStore(pinia).set('showExperiments', true);
    const app = createSSRApp(App);
    app.use(pinia);

    const html = await renderToString(app);

    expect(html).not.toContain('実験的機能');
    expect(html).toContain('環境設定');
  });

  it('表示を有効に残した client が hydrate しても mismatch の警告は出ず、mount の後に項目が出る', async () => {
    const serverApp = createSSRApp(App);
    serverApp.use(persistedPinia());
    const html = await renderToString(serverApp);

    seedSettings({ preferences: { showExperiments: true } });
    const container = document.createElement('div');
    container.innerHTML = html;
    document.body.append(container);
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});

    const clientApp = createSSRApp(App);
    clientApp.use(persistedPinia());
    clientApp.mount(container);

    // 最初の描画（hydrate）は server の HTML と同じ（項目は無い）
    expect(container.textContent).not.toContain('実験的機能');
    await nextTick();
    await nextTick();
    expect(container.textContent).toContain('実験的機能');
    const mismatches = [...warn.mock.calls, ...error.mock.calls].filter((call) =>
      call.join(' ').toLowerCase().includes('hydration'),
    );
    expect(mismatches).toEqual([]);

    clientApp.unmount();
    container.remove();
  });
});
