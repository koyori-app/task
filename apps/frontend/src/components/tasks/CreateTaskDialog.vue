<script setup lang="ts">
import { ChevronDown, X } from '@lucide/vue';
import { computed, ref, watch } from 'vue';
import { useQueryClient } from '@tanstack/vue-query';
import { getActivePinia } from 'pinia';

import HydrationSafeForm from '@/components/HydrationSafeForm.vue';
import { Button, Dialog, Field, Input, Spinner } from '@koyori-app/ui-vue';
import UserSelector from '@/components/users/UserSelector.vue';
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
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
import { apiClient, TASK_SEARCH_PATH } from '@/lib/api-vue-query';
import { PRIORITY_CONFIG } from '@/lib/task-display';
import { toIsoDate } from '@/lib/task-date';
import { useAuthStore } from '@/stores/auth';

const CREATE_TASK_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks' as const;

type Priority = components['schemas']['TaskPriority'];
type Status = components['schemas']['ProjectStatusResponse'];
type LabelOption = components['schemas']['LabelResponse'];
/** 担当者の候補。`assignable-users` が返すのはメンバー行ではなく利用者そのもの */
type MemberOption = components['schemas']['UserSummary'];
type CreatedTask = components['schemas']['TaskDetailResponse'];

/** 担当者に付ける役割。詳細から付けるときと同じ値にする（docs/features/tasks/1.core.md）。 */
const ASSIGNEE_ROLE = 'primary';

/**
 * 見積もり（分）の上限。API の `estimated_minutes` は i32 なので、超えると backend の
 * デシリアライズで落ち、利用者には入力の誤りではなく「作成に失敗しました」しか出ない。
 */
const MAX_ESTIMATE_MINUTES = 2_147_483_647;

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
  /** 開いたときのタイトルの初期値（ドキュメントの選択範囲から作るとき等） */
  initialTitle?: string;
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
const plannedStartOn = ref('');
const softDeadline = ref('');
const hardDeadline = ref('');
/** 見積もり（分）。type="number" の v-model は数値でも文字列でも来る */
const estimate = ref<string | number>('');
const priority = ref<Priority>('Medium');
const selectedLabelIds = ref<string[]>([]);
const selectedAssigneeIds = ref<string[]>([]);
const validationMessage = ref<string | null>(null);
// 文字列（YYYY-MM-DD）同士の比較で足りる。サーバーは前後関係を検証しない
const plannedStartAfterSoft = computed(
  () => !!plannedStartOn.value && !!softDeadline.value && plannedStartOn.value > softDeadline.value,
);
const requestError = ref<string | null>(null);
const successMessage = ref<string | null>(null);

const defaultStatusId = computed(
  () => props.statuses.find((status) => status.is_default)?.id ?? props.statuses[0]?.id ?? '',
);

const selectedLabels = computed(() =>
  (props.labels ?? []).filter((label) => selectedLabelIds.value.includes(label.id)),
);
// 候補で自分を選んだ者の次に置く。pinia の無い所（試験）でも組めるよう、在る時だけ引く。
const authStore = getActivePinia() ? useAuthStore() : null;
const currentUserId = computed(() => authStore?.user?.id ?? null);

const selectedAssignees = computed(() =>
  (props.members ?? []).filter((user) => selectedAssigneeIds.value.includes(user.id)),
);

const createMutation = apiClient.useMutation('post', CREATE_TASK_PATH);
const titleInputRef = ref<InstanceType<typeof Input> | null>(null);

// 開いて入力欄ができたら当てる（× より先に）。koyori の Input は autofocus を root の span に落とすので中の input に当てる
watch(titleInputRef, (input) => {
  (input?.$el as HTMLElement | undefined)?.querySelector('input')?.focus();
});

watch(
  () => [props.open, defaultStatusId.value] as const,
  ([open]) => {
    if (open && !props.statuses.some((status) => status.id === statusId.value)) {
      statusId.value = defaultStatusId.value;
    }
  },
  { immediate: true },
);

// 開くたびに初期値を入れる。閉じると resetForm が空に戻す
watch(
  () => props.open,
  (open) => {
    if (open && props.initialTitle) title.value = props.initialTitle;
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
// onClose 側だけで捨てていたときは、作成に成功して親が open を false にする
// 経路（created を受けてダイアログを閉じる）がここを通らないため、「タスクを
// 作成しました」が残り、次に開いた瞬間に前回の成功が表示されていた。
watch(
  () => props.open,
  (open) => {
    if (!open) resetForm();
  },
);

function onClose() {
  if (createMutation.isPending.value) return;
  // 親が open を握っていない場合でも捨てられるよう、ここでも呼ぶ。
  // resetForm は冪等なので watch と二重に走っても問題ない
  resetForm();
  emit('update:open', false);
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
  plannedStartOn.value = '';
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
  if (plannedStartAfterSoft.value) {
    validationMessage.value = '開始予定日は期限以前の日付にしてください';
    return;
  }
  const normalizedEstimate = String(estimate.value).trim();
  const estimatedMinutes = normalizedEstimate ? Number(normalizedEstimate) : undefined;
  if (
    estimatedMinutes !== undefined &&
    (!Number.isInteger(estimatedMinutes) ||
      estimatedMinutes < 1 ||
      estimatedMinutes > MAX_ESTIMATE_MINUTES)
  ) {
    validationMessage.value = `見積もりは 1 以上 ${MAX_ESTIMATE_MINUTES} 以下の整数（分）で入力してください`;
    return;
  }

  const body: components['schemas']['CreateTaskRequest'] = {
    title: normalizedTitle,
    status_id: statusId.value,
    priority: priority.value,
  };
  const normalizedDescription = description.value.trim();
  if (normalizedDescription) body.description = normalizedDescription;
  if (plannedStartOn.value) body.planned_start_on = plannedStartOn.value;
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
    void Promise.all([
      queryClient.invalidateQueries({ queryKey: ['get', CREATE_TASK_PATH] }),
      queryClient.invalidateQueries({ queryKey: ['get', TASK_SEARCH_PATH] }),
    ]);
    emit('created', created);
    resetForm();
    successMessage.value = 'タスクを作成しました';
  } catch {
    requestError.value = 'タスクの作成に失敗しました。もう一度お試しください';
  }
}
</script>

<template>
  <div class="contents [--koyori-dialog-width:1040px] md:[--koyori-dialog-height:min(720px,90vh)]">
    <!--
      左に本文（タイトル・説明）、右にプロパティを置く 2 列。説明を広く書けるよう
      ダイアログを大きく取り、狭い画面では縦に積んで全体をスクロールさせる。
      見出しは画面に出さない（タイトル欄が見出しの役をする）。plain の title はダイアログの名前として残る
    -->
    <Dialog
      :open="open"
      title="新規タスク"
      :description="`${projectKey} にタスクを追加します`"
      plain
      :on-close="onClose"
    >
      <!--
        閉じるボタンはダイアログの右上に 1 個だけ絶対配置する。右列の中に置くと、
        狭い画面で右列が本文の下に積まれて × が画面の途中に現れる
      -->
      <Button
        type="button"
        variant="ghost"
        class="absolute top-2 right-3 z-10"
        ariaLabel="閉じる"
        :disabled="createMutation.isPending.value"
        @click="onClose"
      >
        <template #icon><X class="size-4" /></template>
      </Button>

      <!-- 本文は開いている間だけ置く（閉じた Dialog も DOM に残るので、Markdown エディタを常駐させない） -->
      <HydrationSafeForm
        v-if="open"
        v-slot="{ isHydrated }"
        class="flex flex-col md:min-h-0 md:flex-1 md:flex-row"
        @submit="submit"
      >
        <div class="flex min-w-0 flex-col md:min-h-0 md:flex-1">
          <!-- 狭い画面ではタイトル欄の右上に × が重なるので、その分だけ右を空ける -->
          <div class="shrink-0 px-5 pt-5 pr-14 md:pr-5">
            <Input
              id="task-title"
              ref="titleInputRef"
              class="font-semibold [--koyori-font-size-sm:1.25rem]"
              aria-label="タイトル（必須）"
              name="title"
              autocomplete="off"
              placeholder="タイトルを入力"
              :value="title"
              :on-value-change="(v: string) => (title = v)"
            />
          </div>

          <div class="flex flex-col gap-3 px-5 py-4 md:min-h-0 md:flex-1 md:overflow-y-auto">
            <!-- CodeMirror の実体は contenteditable で label の for が効かないため、
                 名前は editor 側の aria-label で与える -->
            <MarkdownEditor
              v-model="description"
              aria-label="説明"
              placeholder="説明を入力（Markdown で書けます）"
              :candidates="members ?? []"
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
              :label="createMutation.isPending.value ? '作成中...' : '作成'"
            >
              <template #icon>
                <Spinner v-if="createMutation.isPending.value" hide-label />
              </template>
            </Button>
            <Button
              type="button"
              variant="tertiary"
              :disabled="createMutation.isPending.value"
              label="取り消す"
              @click="onClose"
            />
          </div>
        </div>

        <div class="flex shrink-0 flex-col border-t bg-sidebar md:w-72 md:border-t-0 md:border-l">
          <!-- 広い画面で右上の × が載る余白。狭い画面では × が本文側に載るので出さない -->
          <div class="hidden h-12 shrink-0 border-b md:block" />

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
                <Button
                  type="button"
                  variant="tertiary"
                  label="再試行"
                  @click="emit('retryMembers')"
                />
              </div>
              <UserSelector
                v-else
                v-model="selectedAssigneeIds"
                :users="members ?? []"
                :current-user-id="currentUserId"
                search-label="メンバーを検索"
                empty-text="担当者に指定できる利用者がいません"
                no-match-text="一致するメンバーがいません"
              >
                <template #trigger>
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
                </template>
              </UserSelector>
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
                <Button
                  type="button"
                  variant="tertiary"
                  label="再試行"
                  @click="emit('retryLabels')"
                />
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

            <!-- koyori の Input は date に max を付けない。期限より後かは送信時にも検証する -->
            <Field
              id="task-planned-start"
              label="開始予定日"
              :error="plannedStartAfterSoft ? '期限より後になっています' : ''"
            >
              <Input
                name="planned_start_on"
                type="date"
                :value="plannedStartOn"
                :on-value-change="(v: string) => (plannedStartOn = v)"
              />
            </Field>

            <Field id="task-soft-deadline" label="期限">
              <Input
                name="soft_deadline"
                type="date"
                :value="softDeadline"
                :on-value-change="(v: string) => (softDeadline = v)"
              />
            </Field>

            <Field id="task-hard-deadline" label="最終期限">
              <Input
                name="hard_deadline"
                type="date"
                :value="hardDeadline"
                :on-value-change="(v: string) => (hardDeadline = v)"
              />
            </Field>

            <Field id="task-estimate" label="見積もり">
              <div class="flex items-center gap-2">
                <Input
                  name="estimated_minutes"
                  type="number"
                  :min="1"
                  :max="MAX_ESTIMATE_MINUTES"
                  :step="1"
                  input-mode="numeric"
                  placeholder="未設定"
                  :value="String(estimate)"
                  :on-value-change="(v: string) => (estimate = v)"
                />
                <span class="shrink-0 text-sm text-muted-foreground">分</span>
              </div>
            </Field>
          </div>
        </div>
      </HydrationSafeForm>
    </Dialog>
  </div>
</template>
