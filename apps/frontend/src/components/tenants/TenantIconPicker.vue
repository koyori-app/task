<script setup lang="ts">
import { IconPicker } from '@koyori-app/ui-vue';

import { TENANT_EMOJI_CHOICES } from '@/components/tenants/tenant-icon';

const props = defineProps<{
  /** 画像 URL。空なら絵文字を出す */
  imageUrl: string;
  /** 未設定は null */
  emoji: string | null;
  disabled?: boolean;
}>();

const emit = defineEmits<{
  'update:emoji': [value: string | null];
  'update:imageUrl': [value: string];
}>();

/*
 * 画像のアップロードは専用 API の実装後に使える。それまでは onImageSelect を渡さないので、
 * IconPicker は「画像」のタブを出さない。
 * 絵文字を選ぶと画像は外れ、「なしにする」は絵文字と画像の両方を外す。
 */
function selectEmoji(emoji: string) {
  emit('update:emoji', emoji);
  emit('update:imageUrl', '');
}

function clear() {
  emit('update:emoji', null);
  emit('update:imageUrl', '');
}
</script>

<template>
  <IconPicker
    label="テナントアイコンを変更"
    :image-url="props.imageUrl || undefined"
    :emoji="props.emoji ?? undefined"
    :emojis="TENANT_EMOJI_CHOICES"
    :disabled="props.disabled"
    :on-emoji-change="selectEmoji"
    :on-clear="clear"
  />
</template>
