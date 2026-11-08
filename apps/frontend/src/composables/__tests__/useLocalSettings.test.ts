import { enableAutoUnmount, mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { defineComponent, h, nextTick } from 'vue';
import { useExperimentFlag, usePreference } from '@/composables/useLocalSettings';
import { useExperimentsStore } from '@/stores/experiments';
import { persistedPinia, seedSettings } from '@/components/settings/__tests__/test-pinia';

enableAutoUnmount(afterEach);
beforeEach(() => localStorage.clear());

/** setup の中で読む口を呼び、最初の描画の値と mount の後の値を控える。 */
function probe() {
  const seen: { first: boolean[]; flag?: ReturnType<typeof useExperimentFlag> } = { first: [] };
  const Probe = defineComponent({
    setup() {
      const flag = useExperimentFlag('koyoriArcIntegration');
      const show = usePreference('showExperiments');
      seen.flag = flag;
      seen.first.push(flag.value, show.value);
      return () => h('div', `${flag.value}/${show.value}`);
    },
  });
  const pinia = persistedPinia();
  const wrapper = mount(Probe, { global: { plugins: [pinia] } });
  return { wrapper, seen, pinia };
}

describe('実験的機能のフラグを読む口', () => {
  it('何も残っていなければ false', async () => {
    const { wrapper } = probe();
    await nextTick();

    expect(wrapper.text()).toBe('false/false');
  });

  it('残った値を持っていても、最初の描画は既定値（SSR の HTML と揃う）で、mount の後に残った値になる', async () => {
    seedSettings({
      preferences: { showExperiments: true },
      experiments: { koyoriArcIntegration: true },
    });
    const { wrapper, seen } = probe();

    expect(seen.first).toEqual([false, false]);
    await nextTick();
    expect(wrapper.text()).toBe('true/true');
  });

  it('表示（環境設定）を無効に戻しても、有効にしたフラグは true のまま', async () => {
    seedSettings({
      preferences: { showExperiments: false },
      experiments: { koyoriArcIntegration: true },
    });
    const { wrapper } = probe();
    await nextTick();

    expect(wrapper.text()).toBe('true/false');
  });

  it('store の値が変わると追いつく', async () => {
    const { wrapper, pinia } = probe();
    await nextTick();

    useExperimentsStore(pinia).set('koyoriArcIntegration', true);
    await nextTick();

    expect(wrapper.text()).toBe('true/false');
  });
});
