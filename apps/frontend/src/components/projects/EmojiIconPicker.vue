<script setup lang="ts">
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';

/** デザイン準拠の選択肢 */
const EMOJI_CHOICES = ['📁', '⚙️', '🎨', '🚀', '📊', '🧪', '📦', '🔧', '🌏', '💡', '🧭', '🗂️'];

defineProps<{ modelValue: string | null }>();
const emit = defineEmits<{ 'update:modelValue': [value: string | null] }>();
</script>

<template>
  <DropdownMenu>
    <DropdownMenuTrigger as-child>
      <button
        type="button"
        aria-label="アイコンを選択"
        class="flex size-14 items-center justify-center rounded-[10px] border bg-secondary text-[26px] shadow-sm"
      >
        <span v-if="modelValue">{{ modelValue }}</span>
        <span v-else class="text-sm text-muted-foreground">なし</span>
      </button>
    </DropdownMenuTrigger>
    <DropdownMenuContent align="start" class="w-[236px] p-2">
      <div class="grid grid-cols-6 gap-0.5">
        <button
          v-for="choice in EMOJI_CHOICES"
          :key="choice"
          type="button"
          class="size-[34px] rounded-md text-lg hover:bg-accent"
          :aria-label="`アイコン ${choice}`"
          @click="emit('update:modelValue', choice)"
        >
          {{ choice }}
        </button>
      </div>
      <button
        type="button"
        class="mt-1 w-full rounded-md px-2 py-1.5 text-left text-sm text-muted-foreground hover:bg-accent"
        @click="emit('update:modelValue', null)"
      >
        アイコンなし
      </button>
    </DropdownMenuContent>
  </DropdownMenu>
</template>
