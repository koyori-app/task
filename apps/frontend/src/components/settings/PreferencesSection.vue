<script setup lang="ts">
import { PhInfo } from '@phosphor-icons/vue';
import { computed } from 'vue';
import SettingToggleList from '@/components/settings/SettingToggleList.vue';
import { Button } from '@/components/ui/button';
import { useHydrated } from '@/composables/useHydrated';
import { defaultValues } from '@/lib/local-settings';
import { PREFERENCE_TABLE, type PreferenceKey } from '@/lib/preferences';
import { useExperimentsStore } from '@/stores/experiments';
import { usePreferencesStore } from '@/stores/preferences';

const store = usePreferencesStore();
const experiments = useExperimentsStore();
// localStorage は SSR で読めぬ。mount の後に残った値を出す（それまでは既定値で、server の HTML と揃う）
const ready = useHydrated();

const values = computed(() => (ready.value ? store.values : defaultValues(PREFERENCE_TABLE)));

/** 表示を無効にしたまま、有効のフラグが隠れて残っている件数（有効のままなので、画面で知らせる） */
const hiddenEnabledCount = computed(() =>
  ready.value && !store.values.showExperiments
    ? Object.values(experiments.values).filter(Boolean).length
    : 0,
);

function change(key: string, value: boolean) {
  store.set(key as PreferenceKey, value);
}
</script>

<template>
  <div class="flex flex-col gap-4" data-preferences-section>
    <p class="text-muted-foreground flex items-start gap-2 text-sm" data-device-notice>
      <PhInfo class="mt-0.5 size-4 shrink-0" aria-hidden="true" />
      <span>
        この端末のこのブラウザだけで効く設定です。アカウントには保存されず、ほかの端末や別のブラウザには引き継がれません。
      </span>
    </p>

    <SettingToggleList :table="PREFERENCE_TABLE" :values="values" :ready="ready" @change="change" />

    <p v-if="hiddenEnabledCount > 0" class="text-sm" role="status" data-hidden-experiments-notice>
      実験的機能の表示は無効ですが、有効にした実験的機能が
      {{ hiddenEnabledCount }}
      件、そのまま働いています。切り替えるには、上の「実験的機能を表示する」を有効にしてください。
    </p>

    <div>
      <Button variant="outline" :disabled="!ready" @click="store.resetAll()">
        すべて既定に戻す
      </Button>
    </div>
  </div>
</template>
