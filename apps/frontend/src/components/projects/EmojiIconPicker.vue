<script setup lang="ts">
import { Button, IconPicker } from '@koyori-app/ui-vue';

/** デザイン準拠の選択肢 */
const EMOJI_CHOICES = ['📁', '⚙️', '🎨', '🚀', '📊', '🧪', '📦', '🔧', '🌏', '💡', '🧭', '🗂️'];

defineProps<{ modelValue: string | null }>();
const emit = defineEmits<{ 'update:modelValue': [value: string | null] }>();
</script>

<template>
  <div class="grid w-[236px] justify-items-start gap-1">
    <IconPicker
      class="w-full"
      label="アイコンを選択"
      :emoji="modelValue ?? undefined"
      :emojis="EMOJI_CHOICES"
      :on-emoji-change="(emoji: string) => emit('update:modelValue', emoji)"
    />
    <!-- IconPicker は絵文字を外す操作を持たないので、「アイコンなし」は外に置く -->
    <Button
      v-if="modelValue"
      type="button"
      variant="ghost"
      label="アイコンなし"
      @click="emit('update:modelValue', null)"
    />
  </div>
</template>
