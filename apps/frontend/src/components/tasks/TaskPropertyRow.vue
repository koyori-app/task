<script setup lang="ts">
import type { Component } from 'vue';

/**
 * タスク詳細のプロパティ 1 行（アイコン＋名前 → 値）。
 *
 * モックの詳細画面は、状態・担当・期限などを「左にアイコン付きの名前、右に操作できる
 * 値」という同じ形で並べる。行ごとにカードを積むと縦に伸びて一覧性が落ちるため、
 * この 1 行を単位にして 2 列へ並べる。
 */
defineProps<{
  label: string;
  icon: Component;
  /** 値が未設定のときに薄く出す文言（モックの "Empty" にあたる） */
  emptyText?: string;
  /** 値があるか。false のとき emptyText を出す */
  filled?: boolean;
}>();
</script>

<template>
  <div class="flex min-w-0 items-start gap-3 py-1">
    <!--
      ラベルは高さ 32px の中央に置く。値の側は、1 行目が地の文なら leading-8、部品なら
      min-h-8 items-center にして、1 行目の中心をラベルとそろえる（値が複数行でも 1 行目で合う）。
    -->
    <div
      class="flex h-8 w-28 shrink-0 items-center gap-2 text-sm text-muted-foreground"
      aria-hidden="true"
    >
      <component :is="icon" class="size-4 shrink-0" />
      <span class="truncate">{{ label }}</span>
    </div>
    <div class="min-w-0 flex-1 text-sm">
      <slot />
      <span v-if="filled === false && emptyText" class="leading-8 text-muted-foreground">
        {{ emptyText }}
      </span>
    </div>
  </div>
</template>
