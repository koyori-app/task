import { defineStore } from 'pinia';
import { ref } from 'vue';
import { createSettingsSerializer, defaultValues, type SettingValues } from '@/lib/local-settings';
import { PREFERENCE_TABLE, type PreferenceKey } from '@/lib/preferences';

export type PreferenceValues = SettingValues<typeof PREFERENCE_TABLE>;

/** この端末のこのブラウザだけで効く環境設定。localStorage に残る（アカウントには保存しない）。 */
export const usePreferencesStore = defineStore(
  'preferences',
  () => {
    const values = ref<PreferenceValues>(defaultValues(PREFERENCE_TABLE));

    function set(key: PreferenceKey, value: boolean) {
      values.value[key] = value;
    }

    function resetAll() {
      values.value = defaultValues(PREFERENCE_TABLE);
    }

    return { values, set, resetAll };
  },
  {
    persist: { pick: ['values'], serializer: createSettingsSerializer(PREFERENCE_TABLE) },
  },
);
