<script setup lang="ts">
import { ChevronDown, Loader2, X } from '@lucide/vue';
import { computed, ref, watch } from 'vue';
import { useQueryClient } from '@tanstack/vue-query';
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';

import HydrationSafeForm from '@/components/HydrationSafeForm.vue';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import MarkdownEditor from '@/components/markdown/MarkdownEditor.vue';
import type { components } from '@/generated/api';
import { apiClient } from '@/lib/api-vue-query';
import { PRIORITY_CONFIG } from '@/lib/task-display';
import { toIsoDate } from '@/lib/task-date';

const CREATE_TASK_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks' as const;

type Priority = components['schemas']['TaskPriority'];
type Status = components['schemas']['ProjectStatusResponse'];
type LabelOption = components['schemas']['LabelResponse'];
/** 担当者の候補。`assignable-users` が返すのはメンバー行ではなく利用者そのもの */
type MemberOption = components['schemas']['UserSummary'];
type CreatedTask = components['schemas']['TaskDetailResponse'];

/** 担当者に付ける役割。詳細から付けるときと同じ値にする（docs/features/tasks/1.core.md）。 */
const ASSIGNEE_ROLE = 'primary';

const priorityOptions = Object.entries(PRIORITY_CONFIG) as [
  Priority,
  (typeof PRIORITY_CONFIG)[Priority],
][];

/** 右列の選択欄。Select のトリガーと見た目を揃える */
const fieldTriggerClass =
  'flex h-9 w-full items-center gap-2 rounded-md border bg-background px-3 text-left text-sm shadow-xs outline-none transition-colors hover:bg-accent focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:opacity-50';
const fieldLabelClass = 'text-xs font-normal text-muted-foreground';

const props = defineProps<{
  open: boolean;
  tenantId: string;
  projectId: string;
  projectKey: string;
  statuses: Status[];
  /** undefined は未取得（ロード中・エラー）。正常な 0 件は空配列で渡すこと */
  labels?: LabelOption[];
  labelsLoading?: boolean;
  /** ラベル一覧が手元に無いときだけ true にすること（使えるキャッシュがあれば false） */
  labelsError?: boolean;
  /**
   * 担当者に選べる利用者。undefined は未取得（ロード中・エラー）。正常な 0 人は空配列で渡すこと。
   *
   * 取得先は `assignable-users`。`members` は管理者しか読めないので、担当者を付けられる
   * 権限（WriteTask）しか持たない利用者では候補が引けない
   */
  members?: MemberOption[];
  membersLoading?: boolean;
  /** 候補が手元に無いときだけ true にすること（使えるキャッシュがあれば false。labelsError と同じ約束） */
  membersError?: boolean;
}>();

const emit = defineEmits<{
  'update:open': [value: boolean];
  created: [task: CreatedTask];
  retryLabels: [];
  retryMembers: [];
}>();

const queryClient = useQueryClient();
const title = ref('');
const statusId = ref('');
const description = ref('');
const softDeadline = ref('');
const hardDeadline = ref('');
/** 見積もり（分）。type="number" の v-model は数値でも文字列でも来る */
const estimate = ref<string | number>('');
const priority = ref<Priority>('Medium');
const selectedLabelIds = ref<string[]>([]);
const selectedAssigneeIds = ref<string[]>([]);
const validationMessage = ref<string | null>(null);
const requestError = ref<string | null>(null);
const successMessage = ref<string | null>(null);

const defaultStatusId = computed(
  () => props.statuses.find((status) => status.is_default)?.id ?? props.statuses[0]?.id ?? '',
);

const selectedLabels = computed(() =>
  (props.labels ?? []).filter((label) => selectedLabelIds.value.includes(label.id)),
);
const selectedAssignees = computed(() =>
  (props.members ?? []).filter((user) => selectedAssigneeIds.value.includes(user.id)),
);

const createMutation = apiClient.useMutation('post', CREATE_TASK_PATH);

watch(
  () => [props.open, defaultStatusId.value] as const,
  ([open]) => {
    if (open && !props.statuses.some((status) => status.id === statusId.value)) {
      statusId.value = defaultStatusId.value;
    }
  },
  { immediate: true },
);

// プロジェクトが切り替わったら旧プロジェクトの入力（特にラベル ID）を持ち越さない
watch(
  () => props.projectId,
  () => resetForm(),
);

// ラベル一覧の正常取得後、削除済み ID を選択から落とす。
// undefined はロード中・エラー（一覧が不明）なので選択を保持する
watch(
  () => props.labels,
  (labels) => {
    if (!labels) return;
    const ids = new Set(labels.map((label) => label.id));
    selectedLabelIds.value = selectedLabelIds.value.filter((id) => ids.has(id));
  },
);

// 候補の正常取得後、外れた ID を選択から落とす（ラベルと同じ理由）
watch(
  () => props.members,
  (members) => {
    if (!members) return;
    const ids = new Set(members.map((user) => user.id));
    selectedAssigneeIds.value = selectedAssigneeIds.value.filter((id) => ids.has(id));
  },
);

// 閉じたら入力と結果表示を捨てる。
//
// onOpenChange 側だけで捨てていたときは、作成に成功して親が open を false にする
// 経路（created を受けてダイアログを閉じる）がここを通らないため、「タスクを
// 作成しました」が残り、次に開いた瞬間に前回の成功が表示されていた。
watch(
  () => props.open,
  (open) => {
    if (!open) resetForm();
  },
);

function onOpenChange(value: boolean) {
  if (!value && createMutation.isPending.value) return;
  // 親が open を握っていない場合でも捨てられるよう、ここでも呼ぶ。
  // resetForm は冪等なので watch と二重に走っても問題ない
  if (!value) resetForm();
  emit('update:open', value);
}

function toggleAssignee(userId: string) {
  selectedAssigneeIds.value = selectedAssigneeIds.value.includes(userId)
    ? selectedAssigneeIds.value.filter((id) => id !== userId)
    : [...selectedAssigneeIds.value, userId];
}

function toggleLabel(labelId: string) {
  selectedLabelIds.value = selectedLabelIds.value.includes(labelId)
    ? selectedLabelIds.value.filter((id) => id !== labelId)
    : [...selectedLabelIds.value, labelId];
}

function resetForm() {
  title.value = '';
  statusId.value = defaultStatusId.value;
  description.value = '';
  softDeadline.value = '';
  hardDeadline.value = '';
  estimate.value = '';
  priority.value = 'Medium';
  selectedLabelIds.value = [];
  selectedAssigneeIds.value = [];
  validationMessage.value = null;
  requestError.value = null;
  successMessage.value = null;
}

async function submit() {
  if (createMutation.isPending.value) return;

  const normalizedTitle = title.value.trim();
  validationMessage.value = null;
  requestError.value = null;
  successMessage.value = null;

  if (!normalizedTitle) {
    validationMessage.value = 'タイトルを入力してください';
    return;
  }
  if (!statusId.value) {
    validationMessage.value = 'ステータスを選択してください';
    return;
  }
  const normalizedEstimate = String(estimate.value).trim();
  const estimatedMinutes = normalizedEstimate ? Number(normalizedEstimate) : undefined;
  if (
    estimatedMinutes !== undefined &&
    (!Number.isInteger(estimatedMinutes) || estimatedMinutes < 1)
  ) {
    validationMessage.value = '見積もりは 1 以上の整数（分）で入力してください';
    return;
  }

  const body: components['schemas']['CreateTaskRequest'] = {
    title: normalizedTitle,
    status_id: statusId.value,
    priority: priority.value,
  };
  const normalizedDescription = description.value.trim();
  if (normalizedDescription) body.description = normalizedDescription;
  if (softDeadline.value) body.soft_deadline = toIsoDate(softDeadline.value);
  if (hardDeadline.value) body.hard_deadline = toIsoDate(hardDeadline.value);
  if (estimatedMinutes !== undefined) body.estimated_minutes = estimatedMinutes;
  if (selectedLabelIds.value.length) body.label_ids = selectedLabelIds.value;
  // role は仕様書が使っている primary に揃える（役割を使い分ける UI はまだ無い）
  if (selectedAssigneeIds.value.length) {
    body.assignees = selectedAssigneeIds.value.map((user_id) => ({
      user_id,
      role: ASSIGNEE_ROLE,
    }));
  }

  try {
    const created = await createMutation.mutateAsync({
      params: { path: { tenant_id: props.tenantId, project_id: props.projectId } },
      body,
    });
    void queryClient.invalidateQueries({
      queryKey: ['get', CREATE_TASK_PATH],
      refetchType: 'none',
    });
    emit('created', created);
    resetForm();
    successMessage.value = 'タスクを作成しました';
  } catch {
    requestError.value = 'タスクの作成に失敗しました。もう一度お試しください';
  }
}
</script>

<template>
  <Dialog v-if="open" :open="true" @update:open="onOpenChange">
    <!--
      左に本文（タイトル・説明）、右にプロパティを置く 2 列。説明を広く書けるよう
      ダイアログを大きく取り、狭い画面では縦に積んで全体をスクロールさせる。
    -->
    <DialogContent
      class="flex max-h-[90vh] flex-col gap-0 overflow-y-auto p-0 sm:max-w-[1040px] md:h-[min(720px,90vh)] md:overflow-hidden"
      :show-close-button="false"
    >
      <!-- 見出しは画面に出さない（タイトル欄が見出しの役をする）。ダイアログの名前として残す -->
      <DialogHeader class="sr-only">
        <DialogTitle>新規タスク</DialogTitle>
        <DialogDescription>{{ projectKey }} にタスクを追加します</DialogDescription>
      </DialogHeader>

      <HydrationSafeForm
        v-slot="{ isHydrated }"
        class="flex flex-col md:min-h-0 md:flex-1 md:flex-row"
        @submit="submit"
      >
        <div class="flex min-w-0 flex-col md:min-h-0 md:flex-1">
          <div class="shrink-0 px-5 pt-5">
            <Label for="task-title" class="sr-only">タイトル（必須）</Label>
            <Input
              id="task-title"
              v-model="title"
              name="title"
              autocomplete="off"
              autofocus
              placeholder="タイトルを入力"
              class="h-auto rounded-none border-0 border-b px-0 pb-2 text-xl font-semibold shadow-none focus-visible:border-ring focus-visible:ring-0 md:text-xl"
            />
          </div>

          <div class="flex flex-col gap-3 px-5 py-4 md:min-h-0 md:flex-1 md:overflow-y-auto">
            <!-- CodeMirror の実体は contenteditable で label の for が効かないため、
                 名前は editor 側の aria-label で与える -->
            <MarkdownEditor
              v-model="description"
              aria-label="説明"
              placeholder="説明を入力（Markdown で書けます）"
              min-height-class="min-h-56"
            />

            <p v-if="validationMessage" role="alert" class="text-sm text-destructive">
              {{ validationMessage }}
            </p>
            <p v-if="requestError" role="alert" class="text-sm text-destructive">
              {{ requestError }}
            </p>
            <p v-if="successMessage" role="status" class="text-sm text-emerald-600">
              {{ successMessage }}
            </p>
          </div>

          <div class="flex shrink-0 items-center gap-2 border-t px-5 py-3.5">
            <Button
              type="submit"
              :disabled="createMutation.isPending.value || !isHydrated || !statusId"
            >
              <Loader2 v-if="createMutation.isPending.value" class="mr-2 size-4 animate-spin" />
              {{ createMutation.isPending.value ? '作成中...' : '作成' }}
            </Button>
            <DialogClose as-child>
              <Button type="button" variant="outline" :disabled="createMutation.isPending.value">
                取り消す
              </Button>
            </DialogClose>
          </div>
        </div>

        <div class="flex shrink-0 flex-col border-t bg-sidebar md:w-72 md:border-t-0 md:border-l">
          <div class="flex shrink-0 items-center justify-end border-b px-3 py-2">
            <DialogClose as-child>
              <Button
                type="button"
                variant="ghost"
                size="icon"
                class="size-8"
                aria-label="閉じる"
                :disabled="createMutation.isPending.value"
              >
                <X class="size-4" />
              </Button>
            </DialogClose>
          </div>

          <div class="flex flex-col gap-3 px-3 py-3.5 md:min-h-0 md:flex-1 md:overflow-y-auto">
            <div class="space-y-1">
              <p :class="fieldLabelClass">プロジェクト</p>
              <p class="flex h-9 items-center px-3 text-sm">{{ projectKey }}</p>
            </div>

            <div class="space-y-1">
              <Label for="task-priority" :class="fieldLabelClass">優先度</Label>
              <Select v-model="priority">
                <SelectTrigger id="task-priority" class="w-full bg-background">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem
                    v-for="[value, config] in priorityOptions"
                    :key="value"
                    :value="value"
                  >
                    <component
                      :is="config.icon"
                      class="size-4"
                      :style="{ color: config.color }"
                      aria-hidden="true"
                    />
                    {{ config.label }}
                  </SelectItem>
                </SelectContent>
              </Select>
              <input type="hidden" name="priority" :value="priority" />
            </div>

            <div class="space-y-1">
              <Label for="task-status" :class="fieldLabelClass">
                ステータス <span class="text-destructive">*</span>
              </Label>
              <Select v-model="statusId">
                <SelectTrigger id="task-status" class="w-full bg-background">
                  <SelectValue placeholder="選択してください" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem v-for="status in statuses" :key="status.id" :value="status.id">
                    <span
                      class="inline-block size-2 shrink-0 rounded-full"
                      :style="{ backgroundColor: status.color }"
                      aria-hidden="true"
                    />
                    {{ status.name }}
                  </SelectItem>
                </SelectContent>
              </Select>
              <input type="hidden" name="status_id" :value="statusId" />
            </div>

            <div
              v-if="membersLoading || membersError || members?.length"
              class="space-y-1"
              role="group"
              aria-labelledby="task-assignees-label"
            >
              <p id="task-assignees-label" :class="fieldLabelClass">担当者</p>
              <p v-if="membersLoading" class="flex h-9 items-center text-xs text-muted-foreground">
                メンバーを読み込み中...
              </p>
              <div v-else-if="membersError" class="flex items-center gap-2">
                <p role="alert" class="text-xs text-destructive">メンバーの取得に失敗しました</p>
                <Button type="button" variant="outline" size="sm" @click="emit('retryMembers')">
                  再試行
                </Button>
              </div>
              <DropdownMenu v-else>
                <DropdownMenuTrigger as-child>
                  <button
                    type="button"
                    :class="fieldTriggerClass"
                    aria-labelledby="task-assignees-label task-assignees-value"
                  >
                    <span
                      id="task-assignees-value"
                      class="min-w-0 flex-1 truncate"
                      :class="selectedAssignees.length ? '' : 'text-muted-foreground'"
                    >
                      {{
                        selectedAssignees.length
                          ? selectedAssignees.map((user) => user.username).join('、')
                          : '未割り当て'
                      }}
                    </span>
                    <ChevronDown class="size-4 shrink-0 opacity-50" aria-hidden="true" />
                  </button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="start" class="min-w-56">
                  <!-- 複数選べるので、選んでもメニューを閉じない -->
                  <DropdownMenuCheckboxItem
                    v-for="user in members"
                    :key="user.id"
                    :model-value="selectedAssigneeIds.includes(user.id)"
                    @select="(event: Event) => event.preventDefault()"
                    @update:model-value="toggleAssignee(user.id)"
                  >
                    {{ user.username }}
                  </DropdownMenuCheckboxItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>

            <div
              v-if="labelsLoading || labelsError || labels?.length"
              class="space-y-1"
              role="group"
              aria-labelledby="task-labels-label"
            >
              <p id="task-labels-label" :class="fieldLabelClass">ラベル</p>
              <p v-if="labelsLoading" class="flex h-9 items-center text-xs text-muted-foreground">
                ラベルを読み込み中...
              </p>
              <div v-else-if="labelsError" class="flex items-center gap-2">
                <p role="alert" class="text-xs text-destructive">ラベルの取得に失敗しました</p>
                <Button type="button" variant="outline" size="sm" @click="emit('retryLabels')">
                  再試行
                </Button>
              </div>
              <DropdownMenu v-else>
                <DropdownMenuTrigger as-child>
                  <button
                    type="button"
                    :class="fieldTriggerClass"
                    aria-labelledby="task-labels-label task-labels-value"
                  >
                    <span
                      id="task-labels-value"
                      class="flex min-w-0 flex-1 items-center gap-1 overflow-hidden"
                    >
                      <template v-if="selectedLabels.length">
                        <span
                          v-for="label in selectedLabels"
                          :key="label.id"
                          class="inline-flex shrink-0 items-center gap-1 rounded bg-muted px-1.5 py-0.5 text-xs"
                        >
                          <span
                            class="inline-block size-2 rounded-full"
                            :style="{ backgroundColor: label.color }"
                            aria-hidden="true"
                          />
                          {{ label.name }}
                        </span>
                      </template>
                      <span v-else class="text-muted-foreground">ラベルなし</span>
                    </span>
                    <ChevronDown class="size-4 shrink-0 opacity-50" aria-hidden="true" />
                  </button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="start" class="min-w-56">
                  <DropdownMenuCheckboxItem
                    v-for="label in labels"
                    :key="label.id"
                    :model-value="selectedLabelIds.includes(label.id)"
                    @select="(event: Event) => event.preventDefault()"
                    @update:model-value="toggleLabel(label.id)"
                  >
                    <span
                      class="inline-block size-2.5 shrink-0 rounded-full"
                      :style="{ backgroundColor: label.color }"
                      aria-hidden="true"
                    />
                    {{ label.name }}
                  </DropdownMenuCheckboxItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>

            <div class="space-y-1">
              <Label for="task-soft-deadline" :class="fieldLabelClass">期限</Label>
              <Input
                id="task-soft-deadline"
                v-model="softDeadline"
                name="soft_deadline"
                type="date"
                class="bg-background"
              />
            </div>

            <div class="space-y-1">
              <Label for="task-hard-deadline" :class="fieldLabelClass">最終期限</Label>
              <Input
                id="task-hard-deadline"
                v-model="hardDeadline"
                name="hard_deadline"
                type="date"
                class="bg-background"
              />
            </div>

            <div class="space-y-1">
              <Label for="task-estimate" :class="fieldLabelClass">見積もり</Label>
              <div class="flex items-center gap-2">
                <Input
                  id="task-estimate"
                  v-model="estimate"
                  name="estimated_minutes"
                  type="number"
                  min="1"
                  step="1"
                  inputmode="numeric"
                  placeholder="未設定"
                  class="bg-background"
                />
                <span class="shrink-0 text-sm text-muted-foreground">分</span>
              </div>
            </div>
          </div>
        </div>
      </HydrationSafeForm>
    </DialogContent>
  </Dialog>
</template>
