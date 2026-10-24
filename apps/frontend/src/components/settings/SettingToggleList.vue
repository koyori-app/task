<script setup lang="ts">
import { Switch } from '@koyori-app/ui-vue';
import type { SettingTable } from '@/lib/local-settings';

/**
 * 設定の表を、切り替えと説明の並びにして出す。値は親が渡す（mount の前は既定値）。
 * `ready` が false の間は、まだ残った値を読んでおらぬので触れぬようにする。
 */
const props = defineProps<{
  table: SettingTable;
  values: Record<string, boolean>;
  ready: boolean;
}>();

const emit = defineEmits<{ change: [key: string, value: boolean] }>();

const idOf = (key: string) => `setting-${key}`;
</script>

<template>
  <ul class="flex flex-col divide-y rounded-lg border">
    <li
      v-for="(definition, key) in props.table"
      :key="key"
      class="flex items-start justify-between gap-4 p-4"
      :data-setting="key"
    >
      <div class="flex min-w-0 flex-col gap-1">
        <label :for="idOf(key)" class="text-sm font-medium">{{ definition.name }}</label>
        <p :id="`${idOf(key)}-description`" class="text-muted-foreground text-sm">
          {{ definition.description }}
        </p>
      </div>
      <!-- 行の左に文言を出すので、スイッチ横のラベルは隠す（読み上げ名は label で保つ） -->
      <Switch
        :id="idOf(key)"
        :label="definition.name"
        hideLabel
        :checked="props.values[key]"
        :disabled="!props.ready"
        :ariaDescribedBy="`${idOf(key)}-description`"
        class="mt-0.5"
        :on-checked-change="(value: boolean) => emit('change', key, value)"
      />
    </li>
  </ul>
</template>
