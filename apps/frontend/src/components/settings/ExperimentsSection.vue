<script setup lang="ts">
import { PhFlask } from '@phosphor-icons/vue';
import { computed } from 'vue';
import SettingToggleList from '@/components/settings/SettingToggleList.vue';
import { Button } from '@/components/ui/button';
import { Skeleton } from '@/components/ui/skeleton';
import { useHydrated } from '@/composables/useHydrated';
import { EXPERIMENT_TABLE, type ExperimentKey } from '@/lib/experiments';
import { useExperimentsStore } from '@/stores/experiments';
import { usePreferencesStore } from '@/stores/preferences';

const store = useExperimentsStore();
const preferences = usePreferencesStore();
// localStorage は SSR で読めぬ。mount の前は「どちらとも決まらぬ」場として骨だけを出し、
// server の HTML と最初の描画を揃える。mount の後に、表示の有効・無効で分ける
const ready = useHydrated();

const visible = computed(() => ready.value && preferences.values.showExperiments);

function change(key: string, value: boolean) {
  store.set(key as ExperimentKey, value);
}
</script>

<template>
  <div class="flex flex-col gap-4" data-experiments-section>
    <div v-if="!ready" class="flex flex-col gap-3" aria-busy="true" data-experiments-loading>
      <Skeleton class="h-5 w-2/3" />
      <Skeleton class="h-20 w-full" />
    </div>

    <!-- 表示が無効のまま直に開いた時は、送り返さず、ここが何で・どうすれば使えるかを案内する -->
    <div
      v-else-if="!visible"
      class="flex flex-col items-start gap-3 rounded-lg border border-dashed p-6"
      data-experiments-disabled
    >
      <PhFlask class="text-muted-foreground size-6" aria-hidden="true" />
      <p class="text-sm font-medium">実験的機能の表示が無効です。</p>
      <p class="text-muted-foreground text-sm">
        環境設定で「実験的機能を表示する」を有効にすると、ここで機能ごとに切り替えられます。
      </p>
      <Button as="a" href="/settings/preferences" variant="outline" size="sm">
        環境設定を開く
      </Button>
    </div>

    <template v-else>
      <p class="text-muted-foreground flex items-start gap-2 text-sm" data-device-notice>
        <PhFlask class="mt-0.5 size-4 shrink-0" aria-hidden="true" />
        <span>この端末のこのブラウザだけで有効です。不安定な機能を含みます。</span>
      </p>

      <SettingToggleList
        :table="EXPERIMENT_TABLE"
        :values="store.values"
        :ready="ready"
        @change="change"
      />

      <div>
        <Button variant="outline" @click="store.resetAll()">すべて既定に戻す</Button>
      </div>
    </template>
  </div>
</template>
