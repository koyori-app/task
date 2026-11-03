<script setup lang="ts">
import { Ellipsis, Loader2 } from '@lucide/vue';
import { computed, nextTick, ref } from 'vue';

import { Button, Dropdown } from '@koyori-app/ui-vue';
import CommentBody from '@/components/tasks/CommentBody.vue';
import CommentEditor from '@/components/tasks/CommentEditor.vue';
import UserAvatar from '@/components/UserAvatar.vue';
import { timelineDateTime, timelineTime } from '@/lib/activity-timeline';
import type { MentionCandidate, MentionTable } from '@/lib/comment-mention';
import type { CommentReply, CommentThread } from '@/composables/useTaskComments';

const props = defineProps<{
  /** スレッド先頭・返信のどちらも同じ形で表示する */
  comment: CommentThread | CommentReply;
  /** 本文の `<@利用者ID>` の印から引ける表示情報 */
  mentions?: MentionTable;
  /** 編集で `@` を打った時に出す候補 */
  mentionCandidates?: readonly MentionCandidate[];
  /** このコメントの更新リクエストが進行中 */
  updating?: boolean;
  /** このコメントの削除リクエストが進行中 */
  deleting?: boolean;
  /**
   * ログイン中ユーザーの ID（/v1/auth/me）。未取得なら null。
   * 編集は backend が投稿者本人以外を必ず 403 にするため、編集ボタンは
   * comment.user.id が一致するときだけ出す。削除は backend が投稿者本人に
   * 加えテナントオーナーにも許していて、frontend からはオーナーかどうかを
   * 確実に判定できないため、削除ボタンは全コメントに出したままにする —
   * 可否は backend が判定し、拒否されたら deleteError を拒まれた通りに表示する。
   */
  currentUserId?: string | null;
  /** 本文の札をプロフィールへの link にするテナント（表示用 id。CommentBody 参照） */
  mentionTenant?: string;
  /** このコメントの更新失敗メッセージ（対象コメントの中に出す） */
  updateError?: string | null;
  /** このコメントの削除失敗メッセージ（対象コメントの中に出す） */
  deleteError?: string | null;
  /** 編集の確定。成功で true を返したら編集 UI を閉じる（失敗時は下書きを残す）。 */
  onUpdate: (body: string) => Promise<boolean>;
  /** 削除の確定。成功で true。 */
  onDelete: () => Promise<boolean>;
  /** 編集 UI を開閉するときに前回の失敗表示を消す（返信の onClearReplyError と同型） */
  onClearUpdateError?: () => void;
  /** 削除確認を開閉するときに前回の失敗表示を消す（返信の onClearReplyError と同型） */
  onClearDeleteError?: () => void;
  /** 日付の見出しの下に並ぶときは時刻だけを出す */
  timeOnly?: boolean;
  /** 投稿者のプロフィール画面の URL。渡すと名前をリンクにする */
  profileHref?: (userId: string) => string;
}>();

const editing = ref(false);
const editDraft = ref('');
const confirmingDelete = ref(false);
const editorRef = ref<InstanceType<typeof CommentEditor> | null>(null);

const menuItems = computed(() => [
  ...(props.comment.user.id === props.currentUserId
    ? [{ value: 'edit', label: 'コメントを編集' }]
    : []),
  ...(confirmingDelete.value
    ? []
    : [{ value: 'delete', label: 'コメントを削除', destructive: true }]),
]);

function onMenuSelect(value: string) {
  if (value === 'edit') void startEditing();
  else openDeleteConfirm();
}

async function startEditing() {
  if (props.updating || props.deleting) return;
  props.onClearUpdateError?.();
  editDraft.value = props.comment.body ?? '';
  editing.value = true;
  closeDeleteConfirm();
  await nextTick();
  editorRef.value?.focus();
}

function cancelEditing() {
  props.onClearUpdateError?.();
  editing.value = false;
  editDraft.value = '';
}

function openDeleteConfirm() {
  props.onClearDeleteError?.();
  confirmingDelete.value = true;
}

function closeDeleteConfirm() {
  props.onClearDeleteError?.();
  confirmingDelete.value = false;
}

async function commitEditing() {
  const next = editDraft.value.trim();
  if (!next || next === (props.comment.body ?? '')) {
    cancelEditing();
    return;
  }
  const saved = await props.onUpdate(next);
  if (saved) cancelEditing();
}

async function confirmDelete() {
  const deleted = await props.onDelete();
  if (deleted) confirmingDelete.value = false;
}
</script>

<template>
  <article
    class="group/comment relative flex items-start gap-2 rounded-lg p-2"
    :class="!editing && 'hover:bg-accent'"
    data-task-comment
    :data-comment-id="comment.id"
  >
    <!-- 投稿者のアイコン。avatar_url が無いユーザーは頭文字で描く。名前は隣に字で出す -->
    <UserAvatar :user="comment.user" size="md" decorative />

    <div class="flex min-w-0 flex-1 flex-col gap-0.5">
      <!-- 時刻はホバーの面にも載るので、4.5:1 を保てる text-secondary にする -->
      <header
        class="flex min-h-6 flex-wrap items-center gap-x-1.5 pr-8 text-xs text-(--koyori-color-text-secondary)"
      >
        <component
          :is="profileHref ? 'a' : 'span'"
          :href="profileHref?.(comment.user.id)"
          class="text-sm font-medium text-foreground"
          :class="profileHref && 'hover:underline'"
          data-comment-author
          >{{ comment.user.name }}</component
        >
        <time :datetime="comment.created_at" :title="timelineDateTime(comment.created_at)">{{
          timeOnly ? timelineTime(comment.created_at) : timelineDateTime(comment.created_at)
        }}</time>
        <!-- backend が作成時に created_at と updated_at へ同一の now を入れるため、
             不一致は「編集された」と等価（create_comment の now 一回化とペア） -->
        <span v-if="!comment.is_deleted && comment.updated_at !== comment.created_at">
          (編集済み)
        </span>
        <span v-if="updating || deleting" class="inline-flex items-center">
          <Loader2 class="size-3 animate-spin" aria-hidden="true" />
        </span>
      </header>

      <!-- 操作はメニューに集める。ホバーできない端末では常に出す -->
      <div
        v-if="!comment.is_deleted && !editing && menuItems.length"
        class="absolute top-0 right-0 focus-within:opacity-100 [@media(hover:hover)]:opacity-0 [@media(hover:hover)]:group-hover/comment:opacity-100 [@media(hover:hover)]:focus-within:opacity-100"
      >
        <Dropdown
          top-layer
          variant="ghost"
          ariaLabel="コメントの操作"
          :disabled="updating || deleting"
          :items="menuItems"
          :on-select="onMenuSelect"
        >
          <template #icon><Ellipsis class="size-4" aria-hidden="true" /></template>
        </Dropdown>
      </div>

      <p v-if="comment.is_deleted" class="text-sm text-muted-foreground">削除されたコメント</p>

      <div v-else-if="editing" class="flex flex-col gap-2">
        <CommentEditor
          v-model="editDraft"
          ref="editorRef"
          min-height-class="min-h-20"
          :disabled="updating"
          aria-label="コメントを編集"
          :candidates="mentionCandidates"
          :mentions="mentions"
        />
        <div class="flex justify-end gap-2">
          <Button
            type="button"
            variant="tertiary"
            :disabled="updating"
            label="キャンセル"
            @click="cancelEditing"
          />
          <Button
            type="button"
            :disabled="updating || !editDraft.trim()"
            :label="updating ? '保存中…' : '保存'"
            @click="commitEditing"
          />
        </div>
      </div>

      <!-- CommentBody が KFM 描画と sanitize の唯一の境界になる。 -->
      <CommentBody
        v-else
        :body="comment.body ?? ''"
        :comment-id="comment.id"
        :mentions="mentions"
        :current-user-id="currentUserId"
        :mention-tenant="mentionTenant"
      />

      <div
        v-if="confirmingDelete && !comment.is_deleted && !editing"
        class="flex items-center justify-end gap-2 text-xs"
      >
        <span class="text-muted-foreground">このコメントを削除しますか？</span>
        <Button
          type="button"
          variant="tertiary"
          :disabled="deleting"
          label="キャンセル"
          @click="closeDeleteConfirm"
        />
        <Button
          type="button"
          variant="danger"
          :disabled="deleting"
          :label="deleting ? '削除中…' : '削除する'"
          @click="confirmDelete"
        />
      </div>

      <!-- 失敗は失敗したコメントの中に出す。一覧末尾にまとめると、どの操作が
         失敗したのか分からない -->
      <p v-if="updateError" class="text-xs text-destructive">{{ updateError }}</p>
      <p v-if="deleteError" class="text-xs text-destructive">{{ deleteError }}</p>

      <slot name="footer" />
    </div>
  </article>
</template>
