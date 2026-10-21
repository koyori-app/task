import { defineStore } from 'pinia';
import { ref } from 'vue';
import { EXPERIMENT_TABLE, type ExperimentKey } from '@/lib/experiments';
import { createSettingsSerializer, defaultValues, type SettingValues } from '@/lib/local-settings';

export type ExperimentValues = SettingValues<typeof EXPERIMENT_TABLE>;

/**
 * 実験的機能のフラグ。この端末のこのブラウザだけで効き、localStorage に残る。
 * 環境設定で「実験的機能を表示する」を無効に戻しても、値は消さない。
 */
export const useExperimentsStore = defineStore(
  'experiments',
  () => {
    const values = ref<ExperimentValues>(defaultValues(EXPERIMENT_TABLE));

    function set(key: ExperimentKey, value: boolean) {
      values.value[key] = value;
    }

    function resetAll() {
      values.value = defaultValues(EXPERIMENT_TABLE);
    }

    return { values, set, resetAll };
  },
  {
    persist: { pick: ['values'], serializer: createSettingsSerializer(EXPERIMENT_TABLE) },
  },
);
