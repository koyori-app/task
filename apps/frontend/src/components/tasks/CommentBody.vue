<script setup lang="ts">
import { computed, ref, watchEffect } from 'vue';

import type { MentionTable } from '@/lib/comment-mention';
import { lookup, toPlainText } from '@/lib/comment-mention';
import { KFM_CONTENT_CLASS } from '@/lib/remark-gfm/content-class';
import '@/lib/remark-gfm/style.css';
import '@/lib/remark-koyori-alerts/style.css';
import '@/lib/rehype-starry-night/style.css';
import '@/lib/rehype-kfm-code/style.css';
import '@/lib/remark-kfm-mermaid/style.css';
import '@/lib/remark-kfm-mention/style.css';

/**
 * コメント本文の唯一の描画境界。
 *
 * `v-html` へ渡すのは説明文と同じ markup-renderer が sanitize した HTML だけにする。
 * client 取得のコメントなので描画器は必要になった時に動的 import し、描画中・失敗時は
 * UUID を露出しないプレーンテキストへ倒す。
 */
const props = defineProps<{
  body: string;
  /** 脚注 id を同じ画面の別コメントから分離する決定的な scope */
  commentId: string;
  /** 印の id から引ける表示情報（コメント一覧 API の `mentions`） */
  mentions?: MentionTable;
  currentUserId?: string | null;
}>();

const html = ref<string | null>(null);
let renderVersion = 0;
const plainBody = computed(() => toPlainText(props.body, (id) => lookup(props.mentions ?? {}, id)));

watchEffect(async () => {
  const version = ++renderVersion;
  const source = props.body;
  const commentId = props.commentId;
  const mentionsProp = props.mentions;
  const mentions = mentionsProp ?? {};
  const currentUserId = props.currentUserId ?? null;
  try {
    const { renderDescription } = await import('@/lib/markup-renderer');
    const rendered = await renderDescription(source, {
      scope: `comment-${commentId}`,
      profile: 'comment',
      mentions,
      currentUserId,
    });
    if (version === renderVersion) html.value = rendered;
  } catch {
    if (version === renderVersion) html.value = null;
  }
});
</script>

<template>
  <div
    v-if="html !== null"
    :class="KFM_CONTENT_CLASS"
    class="text-sm leading-relaxed"
    data-comment-body
    v-html="html"
  />
  <p v-else class="whitespace-pre-wrap text-sm leading-relaxed" data-comment-body>
    {{ plainBody }}
  </p>
</template>
