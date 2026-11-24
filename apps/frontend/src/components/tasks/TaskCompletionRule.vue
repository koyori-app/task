<script setup lang="ts">
import { Check } from '@lucide/vue';
import { AvatarGroup, Switch } from '@koyori-app/ui-vue';
import { computed } from 'vue';

import UserAvatar from '@/components/UserAvatar.vue';
import TaskAssigneePicker from '@/components/tasks/TaskAssigneePicker.vue';
import type { components } from '@/generated/api';

/**
 * タスクの担当者の欄。担当者の表示・編集と、「完了条件」（`require_all_assignees_done`）を 1 つにまとめる。
 *
 * 完了条件がオフのときは、重ねたアバター（押すと担当者を選び直す）とスイッチ。
 * オンのタスクは担当者全員が自分の分を完了にして初めて完了になるので、1 人 1 行の縦リストにして、
 * 誰が済んでいて誰がまだかを出す。自分の分の完了・取り消しは自分の行で行う。他の担当者の分は
 * 本人にしか操作できないので、表示だけにする。
 *
 * 状態は色とアイコンで出し、文字（完了 / 未完了、x / y 人）は重ねない。読み上げには文字を渡す。
 */
type TaskDetail = components['schemas']['TaskDetailResponse'];
type Assignee = TaskDetail['assignees'][number];
type Member = { id: string; username: string; avatar_url?: string | null };

/** 自分の行より下で、スクロールせずに出す行数（1 行 28px、6 行で 168px） */
const VISIBLE_ROWS = 6;

const props = defineProps<{
  task: Pick<TaskDetail, 'require_all_assignees_done' | 'assignees'>;
  /** ログイン中の利用者。担当者のときだけ自分の分を操作できる */
  currentUserId?: string | null;
  /** タスクが完了ステータスにいるか。完了済みの間は取り消せない */
  taskDone: boolean;
  /** 完了条件（切り替えと自分の分）の更新中 */
  updating?: boolean;
  error?: string | null;
  /** 担当者に選べるメンバー。渡されなければ担当者は表示だけ */
  members?: Member[];
  /** 候補の取得状態。取得中・失敗を「候補 0 人」と混ぜない */
  membersState?: { loading?: boolean; error?: boolean; onRetry?: () => void };
  /** 担当者の更新中。ピッカーを止める */
  assigneeUpdating?: boolean;
  /** 担当者のプロフィール画面の URL。渡すと担当者の名前をリンクにする */
  profileHref?: (userId: string) => string;
}>();

const emit = defineEmits<{
  'update:requireAll': [value: boolean];
  complete: [];
  reopen: [];
  'toggle:assignee': [userId: string, checked: boolean];
}>();

const requireAll = computed(() => !!props.task.require_all_assignees_done);
const selected = computed(() => props.task.assignees.map((assignee) => assignee.user));
const mine = computed(() =>
  props.currentUserId
    ? props.task.assignees.find((assignee) => assignee.user.id === props.currentUserId)
    : undefined,
);
/** 自分のほかの担当者。「まだ」を上、「済み」を下に並べる（中の順は元のまま） */
const others = computed<Assignee[]>(() => {
  const rest = props.task.assignees.filter((assignee) => assignee !== mine.value);
  return [
    ...rest.filter((assignee) => !assignee.completed_at),
    ...rest.filter((assignee) => !!assignee.completed_at),
  ];
});
const scrollable = computed(() => others.value.length > VISIBLE_ROWS);
</script>

<template>
  <div class="flex flex-col gap-1" data-completion-rule>
    <!-- オフ: 重ねたアバター。メンバーを渡された消費側では押して選び直せる -->
    <template v-if="!requireAll">
      <div class="flex min-h-8 items-center">
        <TaskAssigneePicker
          v-if="members"
          top-layer
          :members="members"
          :selected="selected"
          :disabled="assigneeUpdating"
          :members-state="membersState"
          @toggle="(userId, checked) => emit('toggle:assignee', userId, checked)"
        />
        <AvatarGroup
          v-else-if="task.assignees.length"
          label="担当者"
          :items="
            task.assignees.map(({ user }) => ({
              name: user.username,
              src: user.avatar_url ?? undefined,
            }))
          "
          :max="3"
          :size="28"
        />
        <span v-else class="text-muted-foreground">未設定</span>
      </div>
      <!-- ピッカーのトリガーはボタンなので、リンクは中に入れず横に並べる -->
      <div
        v-if="profileHref && task.assignees.length"
        class="flex flex-wrap gap-x-2 text-xs"
        data-assignee-profile-links
      >
        <a
          v-for="assignee in task.assignees"
          :key="assignee.user.id"
          :href="profileHref(assignee.user.id)"
          class="text-muted-foreground hover:text-foreground hover:underline"
        >
          {{ assignee.user.username }}
        </a>
      </div>
    </template>

    <!-- オン: 1 人 1 行 -->
    <template v-else>
      <p v-if="!task.assignees.length" class="text-xs leading-8 text-muted-foreground">
        担当者を設定すると、全員の完了が必要になります
      </p>
      <div v-else data-completion-assignees>
        <!-- 自分の行はいちばん上に固定する（下の一覧がスクロールしても動かない） -->
        <div
          v-if="mine"
          class="flex h-7 items-center gap-2 pr-2"
          data-completion-mine
          :data-completed="mine.completed_at ? 'true' : 'false'"
        >
          <UserAvatar :user="mine.user" size="sm" decorative />
          <span class="min-w-0 flex-1 truncate font-semibold"
            >{{ mine.user.username }}（自分）</span
          >
          <span class="flex w-6 shrink-0 justify-center">
            <button
              v-if="!mine.completed_at"
              type="button"
              class="inline-flex size-6 items-center justify-center rounded-md border text-muted-foreground transition-colors hover:border-transparent hover:bg-[var(--koyori-color-success)] hover:text-white disabled:opacity-50"
              aria-label="自分の分を完了にする"
              :disabled="updating"
              @click="emit('complete')"
            >
              <Check class="size-3.5" aria-hidden="true" />
            </button>
            <button
              v-else-if="!taskDone"
              type="button"
              class="inline-flex size-6 items-center justify-center rounded-md border border-transparent bg-[var(--koyori-color-success)] text-white transition-opacity hover:opacity-80 disabled:opacity-50"
              aria-label="自分の完了を取り消す"
              :disabled="updating"
              @click="emit('reopen')"
            >
              <Check class="size-3.5" aria-hidden="true" />
            </button>
            <template v-else>
              <Check class="size-4 text-[var(--koyori-color-success)]" aria-hidden="true" />
              <span class="sr-only">済み</span>
            </template>
          </span>
        </div>
        <ul
          v-if="others.length"
          class="assignee-list"
          :class="{ scrollable }"
          :data-completion-scroll="scrollable ? 'true' : undefined"
          :tabindex="scrollable ? 0 : undefined"
          :aria-label="scrollable ? 'ほかの担当者' : undefined"
        >
          <li
            v-for="assignee in others"
            :key="assignee.user.id"
            class="flex h-7 items-center gap-2"
            :class="{ 'pr-2': !scrollable }"
            :data-completed="assignee.completed_at ? 'true' : 'false'"
          >
            <UserAvatar :user="assignee.user" size="sm" decorative />
            <a
              v-if="profileHref"
              :href="profileHref(assignee.user.id)"
              class="min-w-0 flex-1 truncate hover:underline"
            >
              {{ assignee.user.username }}
            </a>
            <span v-else class="min-w-0 flex-1 truncate">{{ assignee.user.username }}</span>
            <span class="flex w-6 shrink-0 justify-center">
              <template v-if="assignee.completed_at">
                <Check class="size-4 text-[var(--koyori-color-success)]" aria-hidden="true" />
                <span class="sr-only">済み</span>
              </template>
              <template v-else>
                <span
                  class="size-3.5 rounded-full border border-dashed border-muted-foreground"
                  aria-hidden="true"
                />
                <span class="sr-only">まだ</span>
              </template>
            </span>
          </li>
        </ul>
      </div>
    </template>

    <div class="flex min-h-8 items-center justify-between gap-2">
      <Switch
        label="全員の完了が必要"
        :checked="requireAll"
        :disabled="updating"
        :on-checked-change="(checked: boolean) => emit('update:requireAll', checked)"
      />
      <!-- 縦リストにはアバターの束が無いので、担当者を選び直す入口をここに置く -->
      <TaskAssigneePicker
        v-if="requireAll && members"
        top-layer
        trigger="icon"
        :members="members"
        :selected="selected"
        :disabled="assigneeUpdating"
        :members-state="membersState"
        @toggle="(userId, checked) => emit('toggle:assignee', userId, checked)"
      />
    </div>

    <p v-if="error" role="alert" class="text-xs text-destructive">{{ error }}</p>
  </div>
</template>

<style scoped>
.assignee-list {
  margin: 0;
  padding: 0;
  list-style: none;
}
/*
 * 6 行（168px）を超えたら、自分の行の下だけスクロールする。スクロールバーの幅 8px を常に空けて、
 * 右端の状態の列を自分の行（pr-2 = 8px）と揃える。下端はぼかして、続きがあることを見せる。
 */
.assignee-list.scrollable {
  max-height: 168px;
  overflow-y: auto;
  scrollbar-gutter: stable;
  mask-image: linear-gradient(to bottom, black calc(100% - 24px), transparent);
}
.assignee-list.scrollable::-webkit-scrollbar {
  width: 8px;
}
.assignee-list.scrollable::-webkit-scrollbar-thumb {
  border-radius: 4px;
  background: var(--koyori-color-border-strong);
}
@supports not selector(::-webkit-scrollbar) {
  .assignee-list.scrollable {
    scrollbar-width: thin;
  }
}
</style>
