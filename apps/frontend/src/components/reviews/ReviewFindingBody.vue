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
 * renderDescription (sanitize 済み KFM HTML) の出力。template の v-html に入れて
 * よいのはこれだけ (markup-renderer の契約。生テキストを v-html へ流す経路を作らない)。
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
    // profile 'comment': 指摘の本文は GitHub の comment 欄の流儀 (空行を強制せず
    // 単一改行で行を分ける) で書かれるため、soft break を <br> として出す
    const rendered = await renderDescription(source, {
      scope: `finding-${props.findingId}`,
      profile: 'comment',
    });
    // 書き込みの競り合いだけを防ぐ: 遅れて終わった古い描画が、後から始まった新しい
    // 本文の出力を上書きしない (成功・失敗の両側で同じ検めを行う)。
    // なお本文が差し替わってから描き上がるまでの間、前の本文の HTML が見え続けるのは
    // 意図である —— 素のテキストへ瞬き戻すより、描き上がりで一度に入れ替える方を採る。
    if (source === props.body) html.value = rendered;
  } catch {
    // 描画に失敗しても本文は読める (素のテキストへ倒す)。古い本文の失敗が
    // いま表示中の描画を消さないよう、成功側と同じ検めを掛ける
    if (source === props.body) html.value = null;
  }
});
</script>

<template>
  <div
    v-if="html !== null"
    :class="KFM_CONTENT_CLASS"
    class="mt-2 text-sm leading-relaxed"
    data-review-finding-body
    v-html="html"
  />
  <p v-else class="mt-2 text-sm whitespace-pre-wrap" data-review-finding-body>{{ body }}</p>
</template>
