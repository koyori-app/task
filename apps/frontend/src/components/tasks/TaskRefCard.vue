<script setup lang="ts">
import { computed } from 'vue';
import { CalendarDays, Flag, Info } from '@lucide/vue';
import UserAvatar from '@/components/UserAvatar.vue';
import LabelMark from '@/components/tasks/LabelMark.vue';
import TaskStatusIcon from '@/components/tasks/TaskStatusIcon.vue';
import { PRIORITY_CONFIG, type ApiPriority } from '@/lib/task-display';
import type { TaskRefCardStatus } from '@/lib/task-ref-card';
/**
 * タスクのホバーカード。取得は親に委ね、ready のときだけタスクの属性を表示する。
 * 状態はアイコンで出し、名前は読み上げにだけ渡す。
 */
const props = defineProps<{
  state: 'loading' | 'ready' | 'unavailable' | 'error';
  seqKey: string;
  href: string;
  title?: string;
  /** 印を字に直した説明。2 行で切る */
  description?: string;
  status?: TaskRefCardStatus | null;
  priority?: ApiPriority | null;
  assignees?: { id: string; username: string; avatar_url?: string | null }[];
  deadline?: { label: string; overdue: boolean } | null;
  labels?: { id: string; name: string; color: string; icon_emoji?: string | null }[];
  archived?: boolean;
}>();
const priorityInfo = computed(() => (props.priority ? PRIORITY_CONFIG[props.priority] : null));
</script>
<template>
  <div class="task-ref-card" data-detail-hover-card data-task-ref-card>
    <div class="head">
      <template v-if="state === 'ready' && status">
        <TaskStatusIcon :status="status" :fill="status.fill" />
        <span class="sr-only">{{ status.name }}</span>
      </template>
      <span class="seq">{{ seqKey }}</span>
      <span v-if="state === 'ready' && archived" class="badge">アーカイブ済み</span>
    </div>
    <template v-if="state === 'ready'">
      <div class="title">{{ title }}</div>
      <p v-if="description" class="description" data-task-ref-description>{{ description }}</p>
      <div class="attributes">
        <div
          v-if="assignees?.length"
          role="group"
          class="attribute"
          :aria-label="`担当者: ${assignees.map((a) => a.username).join(', ')}`"
        >
          <span class="avatars">
            <UserAvatar
              v-for="user in assignees.slice(0, 3)"
              :key="user.id"
              :user="user"
              size="sm"
              decorative
              class="avatar"
            />
          </span>
          <span v-if="assignees.length === 1" class="name">{{ assignees[0].username }}</span>
          <span v-else-if="assignees.length > 3">+{{ assignees.length - 3 }}</span>
        </div>
        <div v-else class="attribute muted">担当者なし</div>
        <div v-if="deadline" class="attribute" :class="{ overdue: deadline.overdue }">
          <CalendarDays :size="14" aria-hidden="true" />{{ deadline.label }}
        </div>
        <div v-if="priorityInfo" class="attribute">
          <Flag :size="14" :style="{ color: priorityInfo.color }" aria-hidden="true" />{{
            priorityInfo.label
          }}
        </div>
      </div>
      <ul v-if="labels?.length" class="labels" aria-label="ラベル">
        <li v-for="label in labels" :key="label.id" class="label">
          <LabelMark :label="label" />{{ label.name }}
        </li>
      </ul>
    </template>
    <p v-else-if="state === 'unavailable'" class="notice">
      <Info :size="14" aria-hidden="true" />タスクが見つからないか、閲覧する権限がありません。
    </p>
    <p v-else-if="state === 'error'">タスクの情報を取得できませんでした。</p>
    <p v-else>読み込み中…</p>
  </div>
</template>
<style scoped>
.task-ref-card {
  box-sizing: border-box;
  padding: 12px 14px;
  display: grid;
  gap: 8px;
  border-radius: var(--koyori-radius-md, 6px);
  color: var(--koyori-color-text);
  font-size: 13px;
  line-height: 18px;
}
.head {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  color: var(--koyori-color-text-secondary);
}
.seq {
  font-family: monospace;
  font-size: 12px;
  overflow-wrap: anywhere;
}
.badge {
  padding: 0 6px;
  border: 1px solid var(--koyori-color-border);
  border-radius: var(--koyori-radius-md, 6px);
  font-size: 11px;
  white-space: nowrap;
}
.title,
.description {
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  overflow-wrap: anywhere;
}
.title {
  font-weight: 600;
  font-size: 15px;
  line-height: 20px;
}
.description {
  color: var(--koyori-color-text-secondary);
  white-space: pre-line;
}
.attributes {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px 12px;
}
.attribute,
.notice {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}
.avatars {
  display: flex;
  align-items: center;
}
.avatars :deep(.avatar + .avatar) {
  margin-left: -6px;
}
.name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.muted {
  color: var(--koyori-color-text-secondary);
}
.labels {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  margin: 0;
  padding: 0;
  list-style: none;
}
.label {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 1px 8px;
  border-radius: var(--koyori-radius-md, 6px);
  background: var(--koyori-color-surface-hover);
  font-size: 12px;
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
</style>
