import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { createSSRApp, h } from 'vue';
import { renderToString } from 'vue/server-renderer';
import ExperimentsSection from '@/components/settings/ExperimentsSection.vue';
import { EXPERIMENT_TABLE } from '@/lib/experiments';
import { useExperimentsStore } from '@/stores/experiments';
import { usePreferencesStore } from '@/stores/preferences';
import { persistedPinia, seedSettings } from './test-pinia';

enableAutoUnmount(afterEach);
beforeEach(() => localStorage.clear());

async function mountSection() {
  const pinia = persistedPinia();
  const wrapper = mount(ExperimentsSection, { global: { plugins: [pinia] } });
  await flushPromises();
  return { wrapper, pinia };
}

describe('ExperimentsSection（実験的機能の画面）: 表示が有効', () => {
  beforeEach(() => seedSettings({ preferences: { showExperiments: true } }));

  it('上に「この端末のこのブラウザだけで有効です。不安定な機能を含みます。」と書く', async () => {
    const { wrapper } = await mountSection();

    expect(wrapper.get('[data-device-notice]').text()).toBe(
      'この端末のこのブラウザだけで有効です。不安定な機能を含みます。',
    );
  });

  it('表のフラグごとに、名前・説明・切り替えを並べる（最初は koyori-arc の統合、既定は無効）', async () => {
    const { wrapper } = await mountSection();

    const rows = wrapper.findAll('[data-setting]');
    expect(rows.map((r) => r.attributes('data-setting'))).toEqual(Object.keys(EXPERIMENT_TABLE));
    const row = wrapper.get('[data-setting="koyoriArcIntegration"]');
    expect(row.text()).toContain('koyori-arc の統合');
    expect(row.text()).toContain(EXPERIMENT_TABLE.koyoriArcIntegration.description);
    expect(row.get('[role="switch"]').attributes('aria-checked')).toBe('false');
  });

  it('切り替えると store と localStorage に残る', async () => {
    const { wrapper, pinia } = await mountSection();

    await wrapper.get('[data-setting="koyoriArcIntegration"] [role="switch"]').trigger('click');

    expect(useExperimentsStore(pinia).values.koyoriArcIntegration).toBe(true);
    expect(JSON.parse(localStorage.getItem('experiments')!).values.koyoriArcIntegration).toBe(true);
  });

  it('「すべて既定に戻す」で全てのフラグが既定値に戻る', async () => {
    seedSettings({ experiments: { koyoriArcIntegration: true } });
    const { wrapper, pinia } = await mountSection();

    await wrapper
      .findAll('button')
      .find((b) => b.text() === 'すべて既定に戻す')!
      .trigger('click');

    expect(useExperimentsStore(pinia).values.koyoriArcIntegration).toBe(false);
    expect(
      wrapper
        .get('[data-setting="koyoriArcIntegration"] [role="switch"]')
        .attributes('aria-checked'),
    ).toBe('false');
  });

  it('表示を無効に戻すと、画面は案内に替わる', async () => {
    const { wrapper, pinia } = await mountSection();

    usePreferencesStore(pinia).set('showExperiments', false);
    await flushPromises();

    expect(wrapper.find('[data-experiments-disabled]').exists()).toBe(true);
    expect(wrapper.find('[role="switch"]').exists()).toBe(false);
  });
});

describe('ExperimentsSection: 表示が無効のまま直に開いた時', () => {
  it('環境設定へ送り返さず、案内と環境設定への link を出す（切り替えは出さぬ）', async () => {
    const { wrapper } = await mountSection();

    const guide = wrapper.get('[data-experiments-disabled]');
    expect(guide.text()).toContain('実験的機能の表示が無効です。');
    expect(guide.text()).toContain('「実験的機能を表示する」を有効にすると');
    expect(guide.get('a').attributes('href')).toBe('/settings/preferences');
    expect(wrapper.find('[role="switch"]').exists()).toBe(false);
    expect(wrapper.find('[data-device-notice]').exists()).toBe(false);
  });
});

describe('ExperimentsSection の SSR', () => {
  it('mount の前は、有効を残していても骨だけを出し、有効とも無効とも決めつけぬ', async () => {
    const pinia = persistedPinia();
    usePreferencesStore(pinia).set('showExperiments', true);
    const app = createSSRApp({ render: () => h(ExperimentsSection) });
    app.use(pinia);

    const html = await renderToString(app);

    expect(html).toContain('data-experiments-loading');
    expect(html).not.toContain('data-experiments-disabled');
    expect(html).not.toContain('role="switch"');
  });

  it('server が骨で描き、残した値を持つ client が hydrate しても mismatch の警告は出ない', async () => {
    const serverApp = createSSRApp({ render: () => h(ExperimentsSection) });
    serverApp.use(persistedPinia());
    const html = await renderToString(serverApp);

    seedSettings({
      preferences: { showExperiments: true },
      experiments: { koyoriArcIntegration: true },
    });
    const container = document.createElement('div');
    container.innerHTML = html;
    document.body.append(container);
    const messages: string[] = [];
    const original = { warn: console.warn, error: console.error };
    console.warn = (...args: unknown[]) => messages.push(args.join(' '));
    console.error = (...args: unknown[]) => messages.push(args.join(' '));
    try {
      const clientApp = createSSRApp({ render: () => h(ExperimentsSection) });
      clientApp.use(persistedPinia());
      clientApp.mount(container);
      await flushPromises();

      expect(container.querySelector('[role="switch"]')?.getAttribute('aria-checked')).toBe('true');
      clientApp.unmount();
    } finally {
      console.warn = original.warn;
      console.error = original.error;
      container.remove();
    }
    expect(messages.filter((m) => m.toLowerCase().includes('hydration'))).toEqual([]);
  });
});
