<script setup lang="ts">
import { Avatar } from '@koyori-app/ui-vue';
import { ArrowRight, Info } from '@lucide/vue';
import { computed } from 'vue';

import { USER_AVATAR_INACTIVE_CLASS } from '@/lib/user-avatar';

/**
 * メンションの札のプロフィールカードの中身。表示だけで、取得はしない（MentionHoverCard が持つ）。
 * カードの面・縁・影・角丸・幅は koyori の HoverCard が持つので、ここでは中身の余白と字だけを指定する。
 * 色は koyori のトークンを直に使う（task の Tailwind の変数は `:root` で解決され、明るい囲みに追従しない）。
 */
const props = defineProps<{
  kind: 'member' | 'outside' | 'gone';
  /** gone では使わぬ（固定の「退会した利用者」） */
  name?: string;
  avatarUrl?: string | null;
  /** 本人への札。「あなた」の印を出す */
  self?: boolean;
  /** 客分。「ゲスト」の印を出す */
  guest?: boolean;
  /** 「役職 · 部署 · チーム」（`profileMetaLine`）。無ければ行ごと出さぬ */
  meta?: string | null;
  bio?: string | null;
  /** 「プロフィールを見る」の先。無ければ下段ごと出さぬ */
  href?: string | null;
}>();

const GONE_NAME = '退会した利用者';

const inactive = computed(() => props.kind !== 'member');
const displayName = computed(() => (props.kind === 'gone' ? GONE_NAME : (props.name ?? '')));
const notice = computed(() => {
  if (props.kind === 'outside')
    return 'このプロジェクトのメンバーではありません。メンションしても通知は届きません。';
  if (props.kind === 'gone') return 'この利用者はテナントから退会しています。';
  return null;
});
const bio = computed(() => (props.kind === 'gone' ? null : props.bio));
const meta = computed(() => (props.kind === 'gone' ? null : props.meta));
const href = computed(() => (props.kind === 'gone' ? null : props.href));
</script>

<template>
  <div class="mention-card">
    <div class="head">
      <Avatar
        :name="kind === 'gone' ? '' : displayName"
        :src="kind === 'gone' ? undefined : avatarUrl || undefined"
        :size="40"
        :class="inactive ? ['is-inactive', USER_AVATAR_INACTIVE_CLASS] : undefined"
        aria-hidden="true"
      />
      <div class="who">
        <div class="name">
          <span class="name-text">{{ displayName }}</span>
          <span v-if="self" class="badge is-self">あなた</span>
          <span v-else-if="guest" class="badge">ゲスト</span>
        </div>
        <div v-if="meta" class="meta">{{ meta }}</div>
      </div>
    </div>
    <div v-if="bio || notice" class="body">
      <p v-if="bio" class="bio">{{ bio }}</p>
      <p v-if="notice" class="notice">
        <Info :size="14" aria-hidden="true" />
        <span>{{ notice }}</span>
      </p>
    </div>
    <div v-if="href" class="foot">
      <a :href="href" class="link">
        プロフィールを見る
        <ArrowRight :size="13" aria-hidden="true" />
      </a>
      <span class="hint">Esc で閉じる</span>
    </div>
  </div>
</template>

<style scoped>
.mention-card {
  color: var(--koyori-color-text);
  font-size: 13px;
  line-height: 18px;
}
.head {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 12px 14px 10px;
}
.who {
  min-width: 0;
}
.name {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 15px;
  line-height: 20px;
  font-weight: 600;
}
.name-text {
  min-width: 0;
  overflow-wrap: anywhere;
}
.badge {
  flex: none;
  padding: 0 6px;
  border: 1px solid var(--koyori-color-border);
  border-radius: 9999px;
  color: var(--koyori-color-text-secondary);
  font-size: 11px;
  line-height: 16px;
  font-weight: 500;
}
.badge.is-self {
  color: var(--koyori-color-mention-self);
  border-color: color-mix(
    in oklab,
    var(--koyori-color-mention-self) 45%,
    var(--koyori-color-surface-raised)
  );
}
.meta {
  color: var(--koyori-color-text-secondary);
  overflow-wrap: anywhere;
}
.body {
  display: grid;
  gap: 8px;
  padding: 0 14px 12px;
}
.body p {
  margin: 0;
}
.bio {
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  overflow-wrap: anywhere;
}
.notice {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  color: var(--koyori-color-text-secondary);
  font-size: 12px;
}
.notice svg {
  flex: none;
  margin-top: 2px;
}
.foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 14px;
  border-top: 1px solid var(--koyori-color-border);
}
.link {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  border-radius: 4px;
  color: var(--koyori-color-text);
  font-size: 12.5px;
  font-weight: 500;
  text-decoration: none;
}
.link:hover {
  text-decoration: underline;
}
.link:focus-visible {
  outline: var(--koyori-focus-width) solid var(--koyori-focus-color);
  outline-offset: var(--koyori-focus-offset);
}
.hint {
  color: var(--koyori-color-text-muted);
  font-size: 11px;
}
</style>
