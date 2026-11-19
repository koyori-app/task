<script setup lang="ts">
import { computed } from 'vue';
import { Info } from '@lucide/vue';
import UserAvatar from '@/components/UserAvatar.vue';
import { PRIORITY_CONFIG, type ApiPriority } from '@/lib/task-display';
/** 取得は親に委ね、ready のときだけタスクの属性を表示する。 */
const props = defineProps<{
  state: 'loading' | 'ready' | 'unavailable' | 'error';
  seqKey: string;
  href: string;
  projectName?: string | null;
  title?: string;
  status?: { name: string; color: string; done: boolean } | null;
  priority?: ApiPriority | null;
  assignees?: { id: string; username: string; avatar_url?: string | null }[];
  deadline?: { label: string; overdue: boolean } | null;
  archived?: boolean;
}>();
const priorityInfo = computed(() => (props.priority ? PRIORITY_CONFIG[props.priority] : null));
</script>
<template>
  <div class="task-ref-card">
    <div class="body">
      <div class="head">
        <span class="seq">{{ seqKey }}</span><span v-if="projectName"> · {{ projectName }}</span
        ><span v-if="state === 'ready' && archived" class="badge">アーカイブ済み</span>
      </div>
      <template v-if="state === 'ready'">
        <div class="title">{{ title }}</div>
        <div v-if="status" class="attribute">
          <span class="dot" :style="{ backgroundColor: status.color }" />{{ status.name }}
        </div>
        <div v-if="priorityInfo" class="attribute">
          <component
            :is="priorityInfo.icon"
            :size="14"
            :style="{ color: priorityInfo.color }"
            aria-hidden="true"
          />{{ priorityInfo.label }}
        </div>
        <div
          v-if="assignees?.length"
          role="group"
          class="attribute"
          :aria-label="`担当者: ${assignees.map((a) => a.username).join(', ')}`"
        >
          <UserAvatar
            v-for="user in assignees.slice(0, 3)"
            :key="user.id"
            :user="user"
            size="sm"
            decorative
          /><span v-if="assignees.length > 3">+{{ assignees.length - 3 }}</span>
        </div>
        <div v-else>担当者なし</div>
        <div v-if="deadline" :class="{ overdue: deadline.overdue }">{{ deadline.label }}</div>
      </template>
      <p v-else-if="state === 'unavailable'" class="notice">
        <Info :size="14" aria-hidden="true" />タスクが見つからないか、閲覧する権限がありません。
      </p>
      <p v-else-if="state === 'error'">タスクの情報を取得できませんでした。</p>
      <p v-else>読み込み中…</p>
    </div>
    <div class="foot">
      <a v-if="state !== 'unavailable'" class="kfm-task-ref" :href="href">タスクを開く</a
      ><span>Esc で閉じる</span>
    </div>
  </div>
</template>
<style scoped>
.task-ref-card {
  color: var(--koyori-color-text);
  font-size: 13px;
  line-height: 18px;
}
.body {
  padding: 12px 14px;
  display: grid;
  gap: 8px;
}
.head {
  color: var(--koyori-color-text-secondary);
  overflow-wrap: anywhere;
}
.seq {
  font-family: monospace;
}
.badge {
  display: inline-block;
  margin-left: 6px;
  padding: 0 6px;
  border: 1px solid var(--koyori-color-border);
  border-radius: 9999px;
  font-size: 11px;
}
.title {
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  overflow-wrap: anywhere;
  font-weight: 600;
  font-size: 15px;
  line-height: 20px;
}
.attribute,
.notice {
  display: flex;
  align-items: center;
  gap: 6px;
}
.dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex: none;
}
p {
  margin: 0;
}
.notice {
  align-items: flex-start;
  color: var(--koyori-color-text-secondary);
}
.notice svg {
  flex: none;
}
.overdue {
  color: var(--koyori-color-danger);
}
.foot {
  display: flex;
  justify-content: space-between;
  padding: 8px 14px;
  border-top: 1px solid var(--koyori-color-border);
}
.foot span {
  color: var(--koyori-color-text-muted);
  font-size: 11px;
}
a {
  color: var(--koyori-color-text);
  text-decoration: none;
}
a:hover {
  text-decoration: underline;
}
a:focus-visible {
  outline: var(--koyori-focus-width) solid var(--koyori-focus-color);
  outline-offset: var(--koyori-focus-offset);
}
</style>
