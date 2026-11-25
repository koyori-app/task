<script setup lang="ts">
/**
 * ラベル名の前に置く印。絵文字を持つラベルは絵文字を、持たないラベルは色の丸を出す。
 * 名前は読み上げられるので、印は装飾として隠す。
 */
defineProps<{ label: { color: string; icon_emoji?: string | null } }>();
</script>

<template>
  <span v-if="label.icon_emoji" aria-hidden="true" class="shrink-0 leading-none">{{
    label.icon_emoji
  }}</span>
  <span
    v-else
    class="inline-block size-2 shrink-0 rounded-full"
    :style="{ backgroundColor: label.color }"
    aria-hidden="true"
  />
</template>
