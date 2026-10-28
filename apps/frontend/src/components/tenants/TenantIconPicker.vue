<script setup lang="ts">
import { IconPicker } from '@koyori-app/ui-vue';

import { TENANT_EMOJI_CHOICES } from '@/components/tenants/tenant-icon';

const props = defineProps<{
  /** 画像 URL。空なら絵文字を出す */
  imageUrl: string;
  emoji: string;
  disabled?: boolean;
}>();

const emit = defineEmits<{
  'update:emoji': [value: string];
  'update:imageUrl': [value: string];
}>();

/*
 * 画像のアップロードは専用 API の実装後に使える。それまでは画像の操作
 * （onImageSelect / onImageRemove / accept）を渡さず、IconPicker は画像のボタンを出さない。
 * 絵文字を選ぶと画像は外れる。
 */
function selectEmoji(emoji: string) {
  emit('update:emoji', emoji);
  emit('update:imageUrl', '');
}
</script>

<template>
  <IconPicker
    class="w-[272px]"
    label="テナントアイコンを変更"
    :image-url="props.imageUrl || undefined"
    :emoji="props.emoji"
    :emojis="TENANT_EMOJI_CHOICES"
    :disabled="props.disabled"
    :on-emoji-change="selectEmoji"
  />
</template>
