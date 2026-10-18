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

/**
 * 描画する本文の長さの上限（説明文の +data.ts の MAX_DESCRIPTION_LENGTH と同値。GitHub issue 本文の上限）。
 * 描画の CPU もキャッシュのメモリも本文長に比例する。backend はコメントの長さを制限せぬので、
 * 超える本文は描かず素の文字へ倒す（保存は無傷）。
 */
const MAX_RENDER_LENGTH = 65_536;

const html = ref<string | null>(null);
let renderVersion = 0;
const plainBody = computed(() => toPlainText(props.body, (id) => lookup(props.mentions ?? {}, id)));

watchEffect(async () => {
  const version = ++renderVersion;
  const source = props.body;
  if (source.length > MAX_RENDER_LENGTH) {
    html.value = null;
    return;
  }
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
    class="kfm-comment text-sm leading-relaxed"
    data-comment-body
    v-html="html"
  />
  <p v-else class="whitespace-pre-wrap text-sm leading-relaxed" data-comment-body>
    {{ plainBody }}
  </p>
</template>

<style>
/*
 * コメントの見出しの大きさ。KFM の見出しは親の字の大きさの em（h1 は 2em）で、説明文では
 * 16px を土台に読みやすいが、14px（text-sm）のコメントでは 28px になり、短い返信の中で
 * 見出しだけが突き出る。コメントの器の中だけ段を詰める（説明文の見た目は変えない）。
 */
.kfm-content.kfm-comment h1 {
  font-size: 1.25em;
}
.kfm-content.kfm-comment h2 {
  font-size: 1.15em;
}
.kfm-content.kfm-comment h3 {
  font-size: 1.05em;
}
.kfm-content.kfm-comment h1,
.kfm-content.kfm-comment h2,
.kfm-content.kfm-comment h3,
.kfm-content.kfm-comment h4,
.kfm-content.kfm-comment h5,
.kfm-content.kfm-comment h6 {
  margin-top: 1rem;
}
.kfm-content.kfm-comment > :first-child {
  margin-top: 0;
}
</style>
