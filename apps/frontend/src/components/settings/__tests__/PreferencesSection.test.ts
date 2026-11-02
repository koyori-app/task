import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { createSSRApp, h } from 'vue';
import { renderToString } from 'vue/server-renderer';
import PreferencesSection from '@/components/settings/PreferencesSection.vue';
import { useExperimentsStore } from '@/stores/experiments';
import { usePreferencesStore } from '@/stores/preferences';
import { persistedPinia, seedSettings } from './test-pinia';

// 絵文字の字の CSS（外の URL）の link を差す。happy-dom は外の CSS を読まず、読まなかったことを
// error として log に出すので、読まなかった CSS を読めた物として扱わせる
(
  window as unknown as { happyDOM: { settings: { handleDisabledFileLoadingAsSuccess: boolean } } }
).happyDOM.settings.handleDisabledFileLoadingAsSuccess = true;

enableAutoUnmount(afterEach);
beforeEach(() => {
  localStorage.clear();
  document.documentElement.style.removeProperty('--koyori-emoji-font');
  document.getElementById('koyori-emoji-font')?.remove();
});

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

    expect(html).toContain('aria-checked="false"');
    expect(html).not.toContain('aria-checked="true"');
    expect(html).toMatch(/<button[^>]*role="switch"[^>]*disabled/);
    // 有効のフラグの知らせも、残った値が要るので SSR では出ない
    expect(html).not.toContain('data-hidden-experiments-notice');
  });

  it('「絵文字の字」の選び肢は OS の既定と Noto で、既定は OS の既定', async () => {
    const { wrapper } = await mountSection();

    const select = wrapper.get<HTMLSelectElement>('[data-setting="emojiFont"] select');
    expect(wrapper.get('[data-setting="emojiFont"]').text()).toContain('絵文字の字');
    expect(select.findAll('option').map((option) => option.text())).toEqual([
      'OS の既定',
      'Noto Color Emoji',
    ]);
    expect(select.element.value).toBe('os');
    expect(document.documentElement.style.getPropertyValue('--koyori-emoji-font')).toBe('');
    expect(document.getElementById('koyori-emoji-font')).toBeNull();
  });

  it('Noto を選ぶと store と localStorage に残り、html に字が当たり、字の CSS を読み込む', async () => {
    const { wrapper, pinia } = await mountSection();

    await wrapper.get('[data-setting="emojiFont"] select').setValue('noto');

    expect(usePreferencesStore(pinia).values.emojiFont).toBe('noto');
    expect(JSON.parse(localStorage.getItem('preferences')!).values.emojiFont).toBe('noto');
    expect(document.documentElement.style.getPropertyValue('--koyori-emoji-font')).toBe(
      "'Noto Color Emoji',",
    );
    expect(document.getElementById('koyori-emoji-font')?.getAttribute('href')).toBe(
      'https://fonts.googleapis.com/css2?family=Noto+Color+Emoji&display=swap',
    );
  });

  it('「すべて既定に戻す」で絵文字の字も OS の既定に戻り、html の字と字の CSS が外れる', async () => {
    seedSettings({ preferences: { emojiFont: 'noto' } });
    const { wrapper, pinia } = await mountSection();
    expect(wrapper.get<HTMLSelectElement>('[data-setting="emojiFont"] select').element.value).toBe(
      'noto',
    );

    await wrapper
      .findAll('button')
      .find((b) => b.text() === 'すべて既定に戻す')!
      .trigger('click');

    expect(usePreferencesStore(pinia).values.emojiFont).toBe('os');
    expect(document.documentElement.style.getPropertyValue('--koyori-emoji-font')).toBe('');
    expect(document.getElementById('koyori-emoji-font')).toBeNull();
  });

  it('選び肢に無い値が残っていたら、OS の既定として読む', async () => {
    seedSettings({ preferences: { emojiFont: 'twemoji', showExperiments: true } });
    const { wrapper, pinia } = await mountSection();

    expect(usePreferencesStore(pinia).values).toEqual({ showExperiments: true, emojiFont: 'os' });
    expect(wrapper.get<HTMLSelectElement>('[data-setting="emojiFont"] select').element.value).toBe(
      'os',
    );
  });

  it('SSR（mount の前）は絵文字の字の選択欄も既定値で描き、触れぬ', async () => {
    const pinia = persistedPinia();
    usePreferencesStore(pinia).set('emojiFont', 'noto');
    const app = createSSRApp({ render: () => h(PreferencesSection) });
    app.use(pinia);

    const html = await renderToString(app);

    expect(html).toMatch(/<select[^>]*id="setting-emojiFont"[^>]*value="os"[^>]*disabled/);
    // 何も選ばれていない select は最初の選び肢を出すので、最初は OS の既定であること
    expect(html).toMatch(/<select[^>]*id="setting-emojiFont"[^>]*>(<!--\[-->)?<option value="os">/);
  });
});
