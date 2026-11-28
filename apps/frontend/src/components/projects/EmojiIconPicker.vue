<script setup lang="ts">
import { IconPicker } from '@koyori-app/ui-vue';

/** 「おすすめ」に出す候補 */
const EMOJI_CHOICES = ['📁', '⚙️', '🎨', '🚀', '📊', '🧪', '📦', '🔧', '🌏', '💡', '🧭', '🗂️'];

defineProps<{ modelValue: string | null }>();
const emit = defineEmits<{ 'update:modelValue': [value: string | null] }>();
</script>

<template>
  <IconPicker
    label="アイコンを選択"
    :emoji="modelValue ?? undefined"
    :emojis="EMOJI_CHOICES"
    :on-emoji-change="(emoji: string) => emit('update:modelValue', emoji)"
    :on-clear="() => emit('update:modelValue', null)"
  />
</template>
