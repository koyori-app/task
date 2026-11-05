<script setup lang="ts">
import { ref } from 'vue';
import { Button, Checkbox } from '@koyori-app/ui-vue';

const props = defineProps<{ codes: string[] }>();
const emit = defineEmits<{ done: [] }>();

const saved = ref(false);
const copyError = ref<string | null>(null);
const copied = ref(false);

async function copy() {
  copyError.value = null;
  try {
    await navigator.clipboard.writeText(props.codes.join('\n'));
    copied.value = true;
  } catch {
    copyError.value = 'コピーできませんでした。コードを書き写して保管してください。';
  }
}
</script>

<template>
  <div class="flex flex-col gap-6">
    <div class="flex flex-col items-center gap-2 text-center">
      <h1 class="text-2xl font-bold">リカバリーコード</h1>
      <p class="text-muted-foreground text-sm text-balance">
        認証アプリを使えなくなったときの代わりのコードです。この画面でしか表示されません。安全な場所に保管してください。
      </p>
    </div>
    <ul
      data-testid="recovery-codes"
      class="grid grid-cols-2 gap-2 rounded-md border bg-secondary p-3 font-mono text-sm"
    >
      <li v-for="code in codes" :key="code">{{ code }}</li>
    </ul>
    <Button
      type="button"
      variant="secondary"
      class="w-full"
      :label="copied ? 'コピーしました' : 'コードをコピー'"
      @click="copy"
    />
    <p v-if="copyError" class="text-destructive text-center text-sm">{{ copyError }}</p>
    <Checkbox
      label="リカバリーコードを保存しました"
      :checked="saved"
      :on-checked-change="(checked: boolean) => (saved = checked)"
    />
    <Button type="button" class="w-full" :disabled="!saved" label="続ける" @click="emit('done')" />
  </div>
</template>
