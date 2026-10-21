import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { createSSRApp, h } from 'vue';
import { renderToString } from 'vue/server-renderer';
import PreferencesSection from '@/components/settings/PreferencesSection.vue';
import { useExperimentsStore } from '@/stores/experiments';
import { usePreferencesStore } from '@/stores/preferences';
import { persistedPinia, seedSettings } from './test-pinia';

enableAutoUnmount(afterEach);
beforeEach(() => localStorage.clear());

async function mountSection() {
  const pinia = persistedPinia();
  const wrapper = mount(PreferencesSection, { global: { plugins: [pinia] } });
  await flushPromises();
  return { wrapper, pinia };
}

describe('PreferencesSection（環境設定の画面）', () => {
  it('この端末のこのブラウザだけで効くことを書く', async () => {
    const { wrapper } = await mountSection();

    expect(wrapper.get('[data-device-notice]').text()).toContain(
      'この端末のこのブラウザだけで効く設定です',
    );
  });

  it('「実験的機能を表示する」の切り替えが在り、既定は無効', async () => {
    const { wrapper } = await mountSection();

    const toggle = wrapper.get('[data-setting="showExperiments"] [role="switch"]');
    expect(wrapper.get('[data-setting="showExperiments"]').text()).toContain(
      '実験的機能を表示する',
    );
    expect(toggle.attributes('aria-checked')).toBe('false');
    expect(toggle.attributes('disabled')).toBeUndefined();
  });

  it('切り替えると store と localStorage に残る', async () => {
    const { wrapper, pinia } = await mountSection();

    await wrapper.get('[data-setting="showExperiments"] [role="switch"]').trigger('click');

    expect(usePreferencesStore(pinia).values.showExperiments).toBe(true);
    expect(JSON.parse(localStorage.getItem('preferences')!).values.showExperiments).toBe(true);
    expect(
      wrapper.get('[data-setting="showExperiments"] [role="switch"]').attributes('aria-checked'),
    ).toBe('true');
  });

  it('残った値は mount の後に画面へ出る', async () => {
    seedSettings({ preferences: { showExperiments: true } });
    const { wrapper } = await mountSection();

    expect(
      wrapper.get('[data-setting="showExperiments"] [role="switch"]').attributes('aria-checked'),
    ).toBe('true');
  });

  it('「すべて既定に戻す」で既定値に戻る', async () => {
    seedSettings({ preferences: { showExperiments: true } });
    const { wrapper, pinia } = await mountSection();

    const reset = wrapper.findAll('button').find((b) => b.text() === 'すべて既定に戻す')!;
    await reset.trigger('click');

    expect(usePreferencesStore(pinia).values.showExperiments).toBe(false);
    expect(
      wrapper.get('[data-setting="showExperiments"] [role="switch"]').attributes('aria-checked'),
    ).toBe('false');
  });

  it('表示を無効にしたまま有効のフラグが残る時は、画面で知らせる', async () => {
    seedSettings({
      preferences: { showExperiments: false },
      experiments: { koyoriArcIntegration: true },
    });
    const { wrapper } = await mountSection();

    expect(wrapper.get('[data-hidden-experiments-notice]').text()).toContain('1 件');
  });

  it('表示が有効の時、またはフラグがどれも無効の時は、知らせを出さない', async () => {
    seedSettings({
      preferences: { showExperiments: true },
      experiments: { koyoriArcIntegration: true },
    });
    const enabled = await mountSection();
    expect(enabled.wrapper.find('[data-hidden-experiments-notice]').exists()).toBe(false);

    localStorage.clear();
    const none = await mountSection();
    expect(none.wrapper.find('[data-hidden-experiments-notice]').exists()).toBe(false);
  });

  it('表示を無効に戻しても、有効にしたフラグの値は消えず、知らせが出る', async () => {
    seedSettings({
      preferences: { showExperiments: true },
      experiments: { koyoriArcIntegration: true },
    });
    const { wrapper, pinia } = await mountSection();

    await wrapper.get('[data-setting="showExperiments"] [role="switch"]').trigger('click');

    expect(useExperimentsStore(pinia).values.koyoriArcIntegration).toBe(true);
    expect(wrapper.find('[data-hidden-experiments-notice]').exists()).toBe(true);
  });

  it('環境設定を既定に戻しても、実験的機能のフラグには触れぬ', async () => {
    seedSettings({
      preferences: { showExperiments: true },
      experiments: { koyoriArcIntegration: true },
    });
    const { wrapper, pinia } = await mountSection();

    await wrapper
      .findAll('button')
      .find((b) => b.text() === 'すべて既定に戻す')!
      .trigger('click');

    expect(useExperimentsStore(pinia).values.koyoriArcIntegration).toBe(true);
  });

  it('SSR（mount の前）は残った値を読まず、既定値で描き、切り替えは触れぬ', async () => {
    const pinia = persistedPinia();
    usePreferencesStore(pinia).set('showExperiments', true);
    const app = createSSRApp({ render: () => h(PreferencesSection) });
    app.use(pinia);

    const html = await renderToString(app);

    expect(html).toContain('data-state="unchecked"');
    expect(html).not.toContain('data-state="checked"');
    expect(html).toMatch(/<button[^>]*role="switch"[^>]*disabled/);
    // 有効のフラグの知らせも、残った値が要るので SSR では出ない
    expect(html).not.toContain('data-hidden-experiments-notice');
  });
});
