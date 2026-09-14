<script setup lang="ts">
import { ref, watchEffect } from 'vue';

// KFM サイドカー CSS の消費契約 (@/lib/markup-renderer/index.ts): v-html する消費側が
// 明示 import する。content-class.ts は leaf module なので KFM コアは entry へ載らない。
import { KFM_CONTENT_CLASS } from '@/lib/remark-gfm/content-class';
import '@/lib/remark-gfm/style.css';
import '@/lib/remark-koyori-alerts/style.css';
import '@/lib/rehype-starry-night/style.css';
import '@/lib/rehype-kfm-code/style.css';
import '@/lib/remark-kfm-mermaid/style.css';

const props = defineProps<{
  /** 指摘の本文 (書き手の入れた markdown。生のまま v-html へ流してはならない) */
  body: string;
  /** 脚注 id (user-content-*) の衝突を避ける scope。指摘 id を渡す */
  findingId: string;
}>();

/**
 * renderDescription (sanitize 済み KFM HTML) の出力。v-html に入れてよいのはこれだけ。
 * SSR とマウント直後は null で、素のテキスト表示へフォールバックする (SSR は
 * このページの指摘一覧が client 取得のため実質通らないが、通っても崩れない)。
 * composition root (@/lib/markup-renderer) は import しただけで KFM 一式が entry へ
 * 載るため、描画が要る時に動的 import する (TaskDetailHub の +data 方式が使えない
 * client 取得ページ側の対応)。
 */
const html = ref<string | null>(null);

watchEffect(async () => {
  const source = props.body;
  try {
    const { renderDescription } = await import('@/lib/markup-renderer');
    const rendered = await renderDescription(source, { scope: `finding-${props.findingId}` });
    // 描画中に本文が差し替わった場合、古い出力を出さない
    if (source === props.body) html.value = rendered;
  } catch {
    // 描画に失敗しても本文は読める (フォールバックのまま)
    html.value = null;
  }
});
</script>

<template>
  <!-- v-html は renderDescription (sanitize 済み) の出力のみ (markup-renderer の契約) -->
  <div
    v-if="html !== null"
    :class="KFM_CONTENT_CLASS"
    class="mt-2 text-sm leading-relaxed"
    data-review-finding-body
    v-html="html"
  />
  <p v-else class="mt-2 text-sm whitespace-pre-wrap" data-review-finding-body>{{ body }}</p>
</template>
