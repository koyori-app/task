import { createPinia, type Pinia } from 'pinia';
import { createPersistedState } from 'pinia-plugin-persistedstate';
import { beforeEach, describe, expect, it } from 'vitest';
import { createApp, nextTick } from 'vue';
import { EXPERIMENT_TABLE } from '@/lib/experiments';
import { PREFERENCE_TABLE } from '@/lib/preferences';
import { useExperimentsStore } from '@/stores/experiments';
import { usePreferencesStore } from '@/stores/preferences';

/**
 * 本番の onCreatePinia と同じく、localStorage に残す pinia を作る。
 * pinia の plugin は app に install されるまで効かぬので、空の app に入れる。
 */
function persistedPinia(): Pinia {
  const pinia = createPinia();
  pinia.use(createPersistedState({ storage: localStorage }));
  createApp({}).use(pinia);
  return pinia;
}

const STORES = [
  {
    label: 'preferences（環境設定）',
    storageKey: 'preferences',
    table: PREFERENCE_TABLE,
    key: 'showExperiments' as const,
    useStore: (pinia: Pinia) => usePreferencesStore(pinia) as unknown as SettingsStore,
  },
  {
    label: 'experiments（実験的機能）',
    storageKey: 'experiments',
    table: EXPERIMENT_TABLE,
    key: 'koyoriArcIntegration' as const,
    useStore: (pinia: Pinia) => useExperimentsStore(pinia) as unknown as SettingsStore,
  },
];

type SettingsStore = {
  values: Record<string, boolean>;
  set(key: string, value: boolean): void;
  resetAll(): void;
};

beforeEach(() => localStorage.clear());

describe.each(STORES)('$label の store', ({ storageKey, table, key, useStore }) => {
  it('既定値は表のとおり（どれも無効）', () => {
    const store = useStore(persistedPinia());

    expect(store.values).toEqual(
      Object.fromEntries(Object.entries(table).map(([k, def]) => [k, def.default])),
    );
    expect(store.values[key]).toBe(false);
  });

  it('切り替えた値が localStorage に values だけ残る（pick）', async () => {
    const store = useStore(persistedPinia());

    store.set(key, true);
    await nextTick();

    const saved = JSON.parse(localStorage.getItem(storageKey)!);
    expect(Object.keys(saved)).toEqual(['values']);
    expect(saved.values[key]).toBe(true);
  });

  it('次の読み込みで、残した値が戻る', async () => {
    useStore(persistedPinia()).set(key, true);
    await nextTick();

    expect(useStore(persistedPinia()).values[key]).toBe(true);
  });

  it('表に無い key の値は、読み込む時に捨てる', () => {
    localStorage.setItem(
      storageKey,
      JSON.stringify({ values: { [key]: true, removedFlag: true } }),
    );

    const store = useStore(persistedPinia());

    expect(store.values[key]).toBe(true);
    expect(store.values).not.toHaveProperty('removedFlag');
    expect(Object.keys(store.values).sort()).toEqual(Object.keys(table).sort());
  });

  it('真偽でない値と壊れた JSON は既定値に倒す', () => {
    localStorage.setItem(storageKey, JSON.stringify({ values: { [key]: 'yes' } }));
    expect(useStore(persistedPinia()).values[key]).toBe(false);

    localStorage.setItem(storageKey, '{broken');
    expect(useStore(persistedPinia()).values[key]).toBe(false);
  });

  it('捨てた key は、次に書く時に localStorage からも消える', async () => {
    localStorage.setItem(
      storageKey,
      JSON.stringify({ values: { [key]: true, removedFlag: true } }),
    );
    const store = useStore(persistedPinia());

    store.set(key, false);
    await nextTick();

    expect(JSON.parse(localStorage.getItem(storageKey)!).values).not.toHaveProperty('removedFlag');
  });

  it('すべて既定に戻すと既定値になり、残る値も既定値になる', async () => {
    const store = useStore(persistedPinia());
    store.set(key, true);
    await nextTick();

    store.resetAll();
    await nextTick();

    expect(store.values[key]).toBe(false);
    expect(JSON.parse(localStorage.getItem(storageKey)!).values[key]).toBe(false);
  });
});

describe('二つの store は互いの値に触れぬ', () => {
  it('環境設定をすべて既定に戻しても、実験的機能のフラグは残る', () => {
    const pinia = persistedPinia();
    const experiments = useExperimentsStore(pinia);
    const preferences = usePreferencesStore(pinia);
    experiments.set('koyoriArcIntegration', true);
    preferences.set('showExperiments', true);

    preferences.resetAll();

    expect(preferences.values.showExperiments).toBe(false);
    expect(experiments.values.koyoriArcIntegration).toBe(true);
  });
});
