<script setup lang="ts">
import { CornerDownLeft, X } from '@lucide/vue';
import { nextTick, onMounted, ref } from 'vue';

import { Button, Input, Spinner } from '@koyori-app/ui-vue';

const props = withDefaults(
  defineProps<{
    onCreate: (title: string) => Promise<boolean>;
    pending?: boolean;
    disabled?: boolean;
    error?: string | null;
    ariaLabel?: string;
    autofocus?: boolean;
  }>(),
  {
    pending: false,
    disabled: false,
    error: null,
    ariaLabel: 'サブタスク名',
    autofocus: true,
  },
);

const emit = defineEmits<{
  cancel: [];
  created: [];
}>();

const draft = ref('');
const inputRef = ref<InstanceType<typeof Input> | null>(null);

// koyori の Input の root は枠の span なので、中の input に当てる
function focusInput() {
  (inputRef.value?.$el as HTMLElement | undefined)?.querySelector('input')?.focus();
}

onMounted(async () => {
  if (!props.autofocus) return;
  await nextTick();
  focusInput();
});

async function submit() {
  const title = draft.value.trim();
  if (!title || props.pending || props.disabled) return;
  if (!(await props.onCreate(title))) return;
  draft.value = '';
  emit('created');
  await nextTick();
  focusInput();
}
</script>

<template>
  <div class="flex flex-col gap-1.5" data-subtask-composer>
    <div class="flex min-w-0 items-center gap-1.5">
      <Input
        ref="inputRef"
        class="min-w-0 flex-1"
        :aria-label="ariaLabel"
        placeholder="サブタスク名を入力"
        :disabled="pending || disabled"
        :value="draft"
        :on-value-change="(v: string) => (draft = v)"
        @keydown.enter.prevent="submit"
        @keydown.esc.prevent="!disabled && emit('cancel')"
      />
      <Button
        type="button"
        :disabled="pending || disabled || !draft.trim()"
        :label="pending ? '追加中…' : '追加'"
        @click="submit"
      >
        <template #icon>
          <Spinner v-if="pending" hide-label :size="14" />
          <CornerDownLeft v-else class="size-3.5" aria-hidden="true" />
        </template>
      </Button>
      <Button
        type="button"
        variant="ghost"
        class="shrink-0"
        :disabled="pending || disabled"
        ariaLabel="サブタスクの追加をやめる"
        @click="emit('cancel')"
      >
        <template #icon><X class="size-4" aria-hidden="true" /></template>
      </Button>
    </div>
    <p v-if="error" class="text-xs text-destructive" role="alert">{{ error }}</p>
  </div>
</template>
