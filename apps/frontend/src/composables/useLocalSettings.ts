import { computed } from 'vue';
import { useHydrated } from '@/composables/useHydrated';
import { EXPERIMENT_TABLE, type ExperimentKey } from '@/lib/experiments';
import { PREFERENCE_TABLE, type PreferenceKey } from '@/lib/preferences';
import { useExperimentsStore } from '@/stores/experiments';
import { usePreferencesStore } from '@/stores/preferences';

/*
 * 環境設定と実験的機能の値を読む口。
 * vike の SSR は localStorage を読めず、server は常に既定値で描く。client が hydrate の最初の描画で
 * 残った値を返すと server の HTML と食い違うので、mount の後（useHydrated が true になってから）に
 * 残った値を返し、それまでは既定値を返す。setup の中（コンポーネント）で呼ぶこと。
 */

/** 環境設定の値。mount の前は既定値。 */
export function usePreference(key: PreferenceKey) {
  const store = usePreferencesStore();
  const hydrated = useHydrated();
  return computed(() => (hydrated.value ? store.values[key] : PREFERENCE_TABLE[key].default));
}

/**
 * 実験的機能のフラグの値。機能の側はここから読む。mount の前は既定値。
 * 環境設定で表示を無効に戻していても、有効にしたフラグはそのまま true を返す。
 */
export function useExperimentFlag(key: ExperimentKey) {
  const store = useExperimentsStore();
  const hydrated = useHydrated();
  return computed(() => (hydrated.value ? store.values[key] : EXPERIMENT_TABLE[key].default));
}
