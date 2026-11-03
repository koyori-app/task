<script setup lang="ts">
import { ArrowLeft, AtSign, SendHorizontal } from '@lucide/vue';
import { computed, nextTick, onBeforeUnmount, onMounted, ref, useTemplateRef, watch } from 'vue';

import CommentEditor from '@/components/tasks/CommentEditor.vue';
import TaskActivityEntry from '@/components/tasks/TaskActivityEntry.vue';
import TaskCommentItem from '@/components/tasks/TaskCommentItem.vue';
import { Button, Skeleton, Spinner } from '@koyori-app/ui-vue';
import { useNow } from '@/composables/useNow';
import type { CommentThread } from '@/composables/useTaskComments';
import { buildTimeline, type ActivityFilter, type ActivityLookups } from '@/lib/activity-timeline';
import type { MentionCandidate, MentionTable } from '@/lib/comment-mention';
import type { ActivityItem } from '@/lib/task-activity';

const props = defineProps<{
  threads: CommentThread[];
  /** 本文の `<@利用者ID>` の印から引ける表示情報（コメント一覧 API の `mentions`） */
  mentions?: MentionTable;
  /** `@` で出す候補（担当者候補と同じ集合。通知の境と一致する） */
  mentionCandidates?: readonly MentionCandidate[];
  /** 一覧の読み込み中 */
  loading?: boolean;
  /**
   * 一覧の読み込み失敗。コメント節の中だけで倒し、ページ本体には影響させない。
   * 投稿（POST）は一覧（GET）と独立に成功しうるため、投稿フォームは出したままにする
   */
  listError?: boolean;
  /** 一覧の再試行（listError 表示の「再試行」から呼ぶ） */
  onRetry?: () => void;
  /**
   * 操作履歴。コメントと別の口で取るので、片方が落ちてももう片方は読み書きできる。
   * 取得の状態もコメントと別に持つ。
   */
  activities?: readonly ActivityItem[];
  activitiesLoading?: boolean;
  activitiesError?: boolean;
  onRetryActivities?: () => void;
  /** まだ取れていない履歴があるか */
  hasMoreActivities?: boolean;
  activitiesFetchingMore?: boolean;
  onLoadMoreActivities?: () => void;
  /** 列の見出しの絞り込み（TaskActivityFilter） */
  filter?: ActivityFilter;
  /** 履歴の ID や名前から、名前と色を引く表 */
  lookups?: ActivityLookups;
  /** ログイン中ユーザーの ID。編集ボタンの出し分けに使う（TaskCommentItem 参照） */
  currentUserId?: string | null;
  /** 投稿者のプロフィール画面の URL（TaskCommentItem 参照） */
  profileHref?: (userId: string) => string;
  /** 本文の札をプロフィールへの link にするテナント（表示用 id。CommentBody 参照） */
  mentionTenant?: string;
  /** 投稿（新規・返信共通）の進行中 */
  submitPending?: boolean;
  /** 新規投稿の失敗。新規投稿フォームの直下に出す */
  submitError?: string | null;
  /**
   * 返信の失敗。`replyErrorThreadId` のスレッドの中にだけ出す。
   *
   * 新規投稿の `submitError` と表示の文脈を分ける（片方の失敗がもう片方の
   * 入力欄の下に出ると、どちらが失敗したのか分からない）。
   */
  replyError?: string | null;
  replyErrorThreadId?: string | null;
  /** 更新リクエスト進行中のコメント ID */
  updatingCommentId?: string | null;
  /** 更新失敗のメッセージと対象コメント ID（当該コメントの中に出す） */
  updateError?: string | null;
  updateErrorCommentId?: string | null;
  /** 削除リクエスト進行中のコメント ID */
  deletingCommentId?: string | null;
  /** 削除失敗のメッセージと対象コメント ID（当該コメントの中に出す） */
  deleteError?: string | null;
  deleteErrorCommentId?: string | null;
  /**
   * 投稿の確定（useTaskComments.submitComment）。成功で true を返したら
   * 下書きを消す。失敗時は下書きを残し、submitError が拒否理由を表示する。
   */
  onSubmit: (body: string, parentCommentId?: string | null) => Promise<boolean>;
  /** 編集の確定（useTaskComments.updateComment） */
  onUpdate: (commentId: string, body: string) => Promise<boolean>;
  /** 削除の確定（useTaskComments.deleteComment） */
  onDelete: (commentId: string) => Promise<boolean>;
  /** 返信フォームを開き直すときに前回の失敗表示を消す（useTaskComments.clearReplyError） */
  onClearReplyError?: () => void;
  /** 編集 UI の開閉で前回の失敗表示を消す（useTaskComments.clearUpdateError） */
  onClearUpdateError?: () => void;
  /** 削除確認の開閉で前回の失敗表示を消す（useTaskComments.clearDeleteError） */
  onClearDeleteError?: () => void;
}>();

/**
 * 開いているスレッドの ID。
 *
 * 参照どおり、返信は一覧の中に折りたたむのではなく列ごとスレッドへ切り替える。
 * 一覧では返信件数だけを出し、押すとその親と返信だけが並ぶ。
 */
const composer = useTemplateRef<InstanceType<typeof CommentEditor>>('composer');

const openThreadId = defineModel<string | null>('openThreadId', { default: null });
const openThread = computed(() => props.threads.find((thread) => thread.id === openThreadId.value));

/**
 * 下書きは「一覧」と「スレッドごと」に分けて持つ。
 *
 * 入力欄は一覧とスレッドで 1 つを共有するので、単一の ref だと行き来した時点で
 * 書きかけが警告なしに消える。既存スレッドを覗くだけ・返信をやめて一覧へ戻るだけの
 * 操作で起きるので、破棄の確認を出すより持ち分けるほうが素直。
 */
const DRAFT_LIST_KEY = '';
const drafts = ref<Record<string, string>>({});
const draftKey = computed(() => openThreadId.value ?? DRAFT_LIST_KEY);
const newDraft = computed({
  get: () => drafts.value[draftKey.value] ?? '',
  set: (value: string) => {
    drafts.value = { ...drafts.value, [draftKey.value]: value };
  },
});

// 開いていたスレッドが消えた（削除・再取得で不在）ら一覧へ戻す
watch(
  () => props.threads,
  (threads) => {
    if (openThreadId.value && !threads.some((thread) => thread.id === openThreadId.value)) {
      openThreadId.value = null;
    }
  },
);

// 絞り込みを変えたらスレッドから一覧へ戻す（スレッドの中は絞り込まない）
watch(
  () => props.filter,
  () => {
    if (openThreadId.value) backToList();
  },
);

const now = useNow();
const sections = computed(() =>
  buildTimeline({
    threads: props.threads,
    activities: props.activities ?? [],
    hasMoreActivities: props.hasMoreActivities,
    filter: props.filter,
    now: now.value,
  }),
);
const showsComments = computed(() => props.filter !== 'history');
const showsHistory = computed(() => props.filter !== 'comments');
const skeletonRows = computed(() => {
  const pending =
    Number(showsComments.value && !!props.loading) +
    Number(showsHistory.value && !!props.activitiesLoading);
  return pending === 2 ? 3 : pending;
});
const emptyMessage = computed(() => {
  if (skeletonRows.value || props.listError || props.activitiesError || sections.value.length) {
    return null;
  }
  if (props.filter === 'comments') return 'コメントはまだありません';
  if (props.filter === 'history') return '履歴はありません';
  return 'アクティビティはまだありません';
});

/**
 * 新しい物が入力欄の隣に来るので、開いた時と増えた時は下端を保つ。
 * 上へ読み進めている間は動かさず、以前の分を足した時は読んでいた位置を保つ。
 * 本文の描画は後から高さが変わるので、中身の高さの変化で合わせ直す。
 */
const feed = useTemplateRef<HTMLElement>('feed');
const feedContent = useTemplateRef<HTMLElement>('feedContent');
let pinnedToBottom = true;
let distanceFromBottom: number | null = null;

function syncScroll() {
  const el = feed.value;
  if (!el) return;
  if (distanceFromBottom !== null) {
    el.scrollTop = el.scrollHeight - distanceFromBottom;
    distanceFromBottom = null;
  } else if (pinnedToBottom) {
    el.scrollTop = el.scrollHeight;
  }
}

function onFeedScroll() {
  const el = feed.value;
  if (el) pinnedToBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
}

function loadMore() {
  const el = feed.value;
  if (el) distanceFromBottom = el.scrollHeight - el.scrollTop;
  props.onLoadMoreActivities?.();
}

watch([sections, openThread], () => void nextTick(syncScroll));
watch([() => props.filter, openThreadId], () => {
  pinnedToBottom = true;
});

let resizeObserver: ResizeObserver | undefined;
onMounted(() => {
  syncScroll();
  if (typeof ResizeObserver === 'undefined' || !feedContent.value) return;
  resizeObserver = new ResizeObserver(syncScroll);
  resizeObserver.observe(feedContent.value);
});
onBeforeUnmount(() => resizeObserver?.disconnect());

/** 入力欄はふだん 1 行にして一覧の場所を空ける。書いている間と下書きがある間は広げる */
const composerFocused = ref(false);
const composerExpanded = computed(() => composerFocused.value || !!newDraft.value.trim());

function onComposerFocusOut(event: FocusEvent) {
  const form = event.currentTarget as HTMLElement;
  if (!form.contains(event.relatedTarget as Node | null)) composerFocused.value = false;
}

/**
 * 下端の入力欄。一覧では新規コメント、スレッドを開いているときはそのスレッドへの返信。
 * 入力欄を 1 つに保つことで、書いている途中に場所を見失わない。
 */
async function submitDraft() {
  const body = newDraft.value.trim();
  if (!body || props.submitPending) return;
  // 送信先と下書きの置き場は**開始時に固定する**。送信中も「コメント一覧へ戻る」は
  // 押せるし、スレッドが消えれば watch が一覧へ戻すので、await の後に
  // openThreadId を読み直すと別の下書きを消すことになる
  const threadId = openThreadId.value;
  const key = threadId ?? DRAFT_LIST_KEY;
  const sent = drafts.value[key] ?? '';
  pinnedToBottom = true;

  const posted = threadId ? await props.onSubmit(body, threadId) : await props.onSubmit(body);
  if (!posted) return;

  // 送った内容がまだ置いてあるときだけ消す。一度離れて同じスレッドへ戻り、
  // 別の下書きを書き始めていたら、それは送信済みの本文ではないので残す
  if (drafts.value[key] !== sent) return;
  const rest = { ...drafts.value };
  delete rest[key];
  drafts.value = rest;
}

function showThread(threadId: string) {
  props.onClearReplyError?.();
  openThreadId.value = threadId;
}

function backToList() {
  props.onClearReplyError?.();
  openThreadId.value = null;
}

async function deleteThread(threadId: string) {
  const deleted = await props.onDelete(threadId);
  // 削除済みスレッドへは返信できない（backend が 400 で弾く）ので一覧へ戻す
  if (deleted && openThreadId.value === threadId) backToList();
  return deleted;
}
</script>

<template>
  <section class="flex min-h-0 flex-1 flex-col" data-task-comments>
    <!--
      アクティビティ列にそのまま収まる形にする（枠で囲わない・見出しは列側が持つ）。
      一覧は伸びる領域として上に、入力欄は列の下端に貼り付ける。
    -->
    <!-- スレッドを開いているときは、列の頭に戻る導線を出す -->
    <div v-if="openThread" class="-mx-4 flex items-center gap-1 px-2 pb-1">
      <Button type="button" variant="ghost" ariaLabel="コメント一覧へ戻る" @click="backToList">
        <template #icon><ArrowLeft class="size-4" aria-hidden="true" /></template>
      </Button>
      <p class="min-w-0 truncate text-sm font-medium">{{ openThread.user.name }} のスレッド</p>
    </div>

    <!--
      少ないときは下端へ寄せる。justify-end は上へ溢れた分へスクロールできないブラウザーがあるので、
      中身の margin-top: auto で寄せる。
    -->
    <div
      ref="feed"
      class="-mx-4 flex min-h-0 flex-1 flex-col overflow-y-auto p-2"
      data-activity-feed
      @scroll.passive="onFeedScroll"
    >
      <div ref="feedContent" class="mt-auto flex flex-col gap-0.5">
        <!-- スレッド表示。列ごと切り替える -->
        <template v-if="openThread">
          <TaskCommentItem
            :comment="openThread"
            :current-user-id="currentUserId"
            :mentions="mentions"
            :mention-candidates="mentionCandidates"
            :profile-href="profileHref"
            :mention-tenant="mentionTenant"
            :updating="updatingCommentId === openThread.id"
            :deleting="deletingCommentId === openThread.id"
            :update-error="updateErrorCommentId === openThread.id ? updateError : null"
            :delete-error="deleteErrorCommentId === openThread.id ? deleteError : null"
            :on-update="(body) => onUpdate(openThread!.id, body)"
            :on-delete="() => deleteThread(openThread!.id)"
            :on-clear-update-error="onClearUpdateError"
            :on-clear-delete-error="onClearDeleteError"
          />

          <div v-if="openThread.replies.length" class="flex items-center gap-2 px-2 pt-2 pb-1">
            <span class="shrink-0 text-xs text-muted-foreground">
              {{ openThread.replies.length }}件の返信
            </span>
            <span class="h-px flex-1 bg-border" aria-hidden="true" />
          </div>

          <TaskCommentItem
            v-for="reply in openThread.replies"
            :key="reply.id"
            :comment="reply"
            :current-user-id="currentUserId"
            :mentions="mentions"
            :mention-candidates="mentionCandidates"
            :profile-href="profileHref"
            :mention-tenant="mentionTenant"
            :updating="updatingCommentId === reply.id"
            :deleting="deletingCommentId === reply.id"
            :update-error="updateErrorCommentId === reply.id ? updateError : null"
            :delete-error="deleteErrorCommentId === reply.id ? deleteError : null"
            :on-update="(body) => onUpdate(reply.id, body)"
            :on-delete="() => onDelete(reply.id)"
            :on-clear-update-error="onClearUpdateError"
            :on-clear-delete-error="onClearDeleteError"
          />

          <p
            v-if="replyError && replyErrorThreadId === openThread.id"
            class="px-2 text-xs text-destructive"
          >
            {{ replyError }}
          </p>
        </template>

        <template v-else>
          <!-- 取得の失敗はそれぞれの口の分だけ倒す。読めているほうは出し続ける -->
          <div
            v-if="showsHistory && activitiesError"
            class="flex items-center justify-between gap-2 px-2 py-1"
          >
            <p class="text-xs text-destructive">履歴を読み込めませんでした</p>
            <Button
              v-if="onRetryActivities"
              type="button"
              variant="tertiary"
              label="再試行"
              @click="onRetryActivities"
            />
          </div>
          <div
            v-if="showsComments && listError"
            class="flex items-center justify-between gap-2 px-2 py-1"
          >
            <p class="text-xs text-destructive">コメントを読み込めませんでした</p>
            <Button
              v-if="onRetry"
              type="button"
              variant="tertiary"
              label="再試行"
              @click="onRetry"
            />
          </div>

          <div v-if="showsHistory && hasMoreActivities && onLoadMoreActivities" class="flex px-2">
            <Button
              type="button"
              variant="ghost"
              :disabled="activitiesFetchingMore"
              :label="activitiesFetchingMore ? '読み込み中…' : '以前のアクティビティを見る'"
              @click="loadMore"
            />
          </div>

          <section
            v-for="section in sections"
            :key="section.key"
            :aria-label="section.label"
            data-activity-section
          >
            <div class="flex items-center gap-2 px-2 pt-3 pb-1">
              <h3 class="shrink-0 text-xs font-normal text-muted-foreground">
                {{ section.label }}
              </h3>
              <span class="h-px flex-1 bg-border" aria-hidden="true" />
            </div>
            <ol class="flex flex-col gap-0.5">
              <li v-for="entry in section.entries" :key="entry.id">
                <TaskActivityEntry
                  v-if="entry.kind === 'history'"
                  :entry="entry"
                  :lookups="lookups"
                />
                <TaskCommentItem
                  v-else
                  time-only
                  :comment="entry.thread"
                  :current-user-id="currentUserId"
                  :mentions="mentions"
                  :mention-candidates="mentionCandidates"
                  :profile-href="profileHref"
                  :mention-tenant="mentionTenant"
                  :updating="updatingCommentId === entry.thread.id"
                  :deleting="deletingCommentId === entry.thread.id"
                  :update-error="updateErrorCommentId === entry.thread.id ? updateError : null"
                  :delete-error="deleteErrorCommentId === entry.thread.id ? deleteError : null"
                  :on-update="(body) => onUpdate(entry.thread.id, body)"
                  :on-delete="() => deleteThread(entry.thread.id)"
                  :on-clear-update-error="onClearUpdateError"
                  :on-clear-delete-error="onClearDeleteError"
                >
                  <!--
                    削除済みスレッドには返信できない（backend の create_comment が 400 で弾く）が、
                    既にある返信は残るので、返信があるときは開く導線を残す。ここを隠すと
                    返信がデータ上は生きているのに読むことも消すこともできなくなる。
                  -->
                  <template v-if="!entry.thread.is_deleted || entry.thread.replies.length" #footer>
                    <button
                      type="button"
                      class="-ml-1 inline-flex h-6 w-fit touch-manipulation items-center rounded-md px-1 text-xs font-medium text-link select-none hover:underline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring active:bg-(--koyori-color-surface-active)"
                      @click="showThread(entry.thread.id)"
                    >
                      {{
                        entry.thread.replies.length
                          ? `${entry.thread.replies.length}件の返信`
                          : '返信'
                      }}
                    </button>
                  </template>
                </TaskCommentItem>
              </li>
            </ol>
          </section>

          <template v-if="skeletonRows">
            <p class="sr-only" role="status">読み込み中</p>
            <div
              v-for="row in skeletonRows"
              :key="row"
              class="flex items-start gap-2 px-2 py-2"
              data-activity-skeleton
              aria-hidden="true"
            >
              <Skeleton width="24px" height="24px" radius="50%" />
              <Skeleton class="min-w-0 flex-1" :lines="2" />
            </div>
          </template>

          <p v-if="emptyMessage" class="px-2 py-1 text-sm text-muted-foreground">
            {{ emptyMessage }}
          </p>
        </template>
      </div>
    </div>

    <!-- 入力欄。下端に貼り付ける。押せない操作は置かない -->
    <p
      v-if="openThread?.is_deleted"
      class="-mx-4 mt-auto shrink-0 border-t bg-background px-4 py-3 text-sm text-muted-foreground"
    >
      削除されたコメントには返信できません
    </p>
    <form
      v-else
      class="-mx-4 mt-auto shrink-0 border-t bg-background px-4 py-3"
      @submit.prevent="submitDraft"
      @focusin="composerFocused = true"
      @focusout="onComposerFocusOut"
    >
      <div
        class="rounded-lg border border-input bg-background focus-within:outline-2 focus-within:outline-offset-2 focus-within:outline-ring"
      >
        <CommentEditor
          ref="composer"
          v-model="newDraft"
          bare
          :min-height-class="composerExpanded ? 'min-h-14' : 'min-h-9'"
          :disabled="submitPending"
          :aria-label="openThread ? '返信を入力' : 'コメントを入力'"
          :placeholder="openThread ? '返信を入力…' : 'コメントを入力…'"
          :candidates="mentionCandidates"
          :mentions="mentions"
        />
        <div v-show="composerExpanded" class="flex items-center justify-between gap-1 p-0.5">
          <Button
            type="button"
            variant="ghost"
            ariaLabel="メンション"
            :disabled="submitPending"
            @click="composer?.openMentionPicker()"
          >
            <template #icon><AtSign class="size-4" aria-hidden="true" /></template>
          </Button>
          <Button
            type="submit"
            :ariaLabel="submitPending ? '送信中' : openThread ? '返信する' : 'コメントする'"
            :disabled="submitPending || !newDraft.trim()"
          >
            <template #icon>
              <Spinner v-if="submitPending" hide-label />
              <SendHorizontal v-else class="size-4" aria-hidden="true" />
            </template>
          </Button>
        </div>
      </div>
      <!--
        新規投稿の失敗は一覧の文脈でだけ出す。入力欄は一覧とスレッドで共有なので、
        そのまま出すとスレッドを開いた瞬間「まだ送っていない返信が失敗した」ように見える。
        消さずに隠すのは、下書きと同じで一覧へ戻れば理由がまた読めるようにするため。
      -->
      <p v-if="submitError && !openThread" class="mt-1 text-xs text-destructive">
        {{ submitError }}
      </p>
    </form>
  </section>
</template>
