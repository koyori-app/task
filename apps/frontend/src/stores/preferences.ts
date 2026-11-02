import { defineStore } from 'pinia';
import { ref, watch } from 'vue';
import { applyEmojiFont } from '@/lib/emoji-font';
import { createSettingsSerializer, defaultValues, type SettingValues } from '@/lib/local-settings';
import { PREFERENCE_TABLE, type PreferenceKey } from '@/lib/preferences';

export type PreferenceValues = SettingValues<typeof PREFERENCE_TABLE>;

/** この端末のこのブラウザだけで効く環境設定。localStorage に残る（アカウントには保存しない）。 */
export const usePreferencesStore = defineStore(
  'preferences',
  () => {
    const values = ref<PreferenceValues>(defaultValues(PREFERENCE_TABLE));

    function set<K extends PreferenceKey>(key: K, value: PreferenceValues[K]) {
      values.value[key] = value;
    }

    // 絵文字の字は html の CSS 変数と、字の CSS の link で当てる。最初の値は描く前に head の script が当てる
    // （emojiFontBootScript）ので、ここでは変えた時と既定に戻した時だけを追う。SSR では document が無い
    if (typeof document !== 'undefined') {
      watch(
        () => values.value.emojiFont,
        (value) => applyEmojiFont(document, value),
      );
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
