<script setup lang="ts">
import { computed, ref } from 'vue';
import { Button, Field, Input } from '@koyori-app/ui-vue';
import HydrationSafeForm from '@/components/HydrationSafeForm.vue';

withDefaults(
  defineProps<{
    pending: boolean;
    error: string | null;
    locked: boolean;
    /** 設定画面に埋め込むとき true。見出しと説明を出さない */
    embedded?: boolean;
    /** false なら TOTP のみ（リカバリーコードへの切替を出さない） */
    allowRecovery?: boolean;
    submitLabel?: string;
  }>(),
  { embedded: false, allowRecovery: true, submitLabel: '確認する' },
);
const emit = defineEmits<{
  submit: [input: { code: string } | { recovery_code: string }];
}>();

/** TOTP とリカバリーコードのどちらで確かめるか。 */
const useRecovery = ref(false);
const value = ref('');

const label = computed(() => (useRecovery.value ? 'リカバリーコード' : '認証アプリのコード'));

function toggle() {
  useRecovery.value = !useRecovery.value;
  value.value = '';
}

function onSubmit() {
  const trimmed = value.value.trim();
  if (!trimmed) return;
  emit('submit', useRecovery.value ? { recovery_code: trimmed } : { code: trimmed });
}
</script>

<template>
  <HydrationSafeForm v-slot="{ isHydrated }" @submit="onSubmit">
    <div class="flex flex-col gap-6">
      <div v-if="!embedded" class="flex flex-col items-center gap-2 text-center">
        <h1 class="text-2xl font-bold">2 段階認証</h1>
        <p class="text-muted-foreground text-sm text-balance">
          {{
            useRecovery
              ? 'リカバリーコードを 1 つ入力してください。使ったコードは再利用できません。'
              : '認証アプリに表示されている 6 桁のコードを入力してください。'
          }}
        </p>
      </div>
      <Field id="two-factor-code" :label="label" :error="error ?? ''">
        <Input
          name="code"
          :placeholder="useRecovery ? 'XXXX-XXXX-XXXX' : '123456'"
          autocomplete="one-time-code"
          :value="value"
          :on-value-change="(v: string) => (value = v)"
        />
      </Field>
      <Button
        type="submit"
        class="w-full"
        :disabled="pending || locked || !isHydrated"
        :label="pending ? '確認中…' : submitLabel"
      />
      <button
        v-if="allowRecovery"
        type="button"
        class="text-muted-foreground text-center text-sm underline underline-offset-4"
        @click="toggle"
      >
        {{ useRecovery ? '認証アプリのコードを使う' : 'リカバリーコードを使う' }}
      </button>
    </div>
  </HydrationSafeForm>
</template>
