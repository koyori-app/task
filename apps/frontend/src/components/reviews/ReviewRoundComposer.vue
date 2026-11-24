<script setup lang="ts">
import { PhPlus, PhX } from '@phosphor-icons/vue';
import { computed, ref, watch } from 'vue';
import { Button, ConfirmDialog, Field, Input, Picker, Textarea } from '@koyori-app/ui-vue';
import { useCreateReviewMutation } from '@/lib/api-vue-query';
import { SEVERITIES, SEVERITY_LABELS, type FindingSeverity } from '@/lib/review-findings';

const props = defineProps<{
  tenantId: string;
  projectId: string;
  prNumber: number;
  /** これから作るラウンドの番号（表示だけに使う。採番はサーバー） */
  nextRound: number;
}>();

const emit = defineEmits<{ created: []; cancel: []; 'dirty-change': [dirty: boolean] }>();

type StagedFinding = {
  severity: FindingSeverity;
  title: string;
  body: string;
  file: string;
  line: string;
};

const createReview = useCreateReviewMutation();
const severityItems = SEVERITIES.map((value) => ({ value, label: SEVERITY_LABELS[value] }));
const SEVERITY_CLASS: Record<FindingSeverity, string> = {
  high: 'text-(--koyori-color-danger)',
  medium: 'text-(--koyori-color-warning)',
  low: 'text-muted-foreground',
  nit: 'text-muted-foreground',
};

const headSha = ref('');
const summary = ref('');
/**
 * 下書き。**確定するまでサーバーには何も作らない**（ラウンドは確定時に
 * 指摘ごと一括作成し、確定後は追記できない。仕様 §3）。
 */
const staged = ref<StagedFinding[]>([]);
const submitError = ref<string | null>(null);

const draft = ref<StagedFinding>(emptyDraft());
const draftError = ref<string | null>(null);

function emptyDraft(): StagedFinding {
  return { severity: 'medium', title: '', body: '', file: '', line: '' };
}

/**
 * commit SHA（40 桁の小文字 16 進）。CLI の `reviews.ts` と同じ形で、backend の
 * `COMMIT_SHA_REGEX` と対。
 *
 * ゲートは `latest_head_sha` を厳密一致で比べるので、短縮 SHA で確定したラウンドは
 * 指摘を全部解消しても通らなくなる。`git log --oneline` が見せるのは短縮 SHA なので
 * 画面から貼るときに起こりやすく、サーバーの 400 は「どの項目が何桁必要か」を
 * 伝えられないため、ここで形を見る。
 */
const COMMIT_SHA = /^[0-9a-f]{40}$/;

const headShaError = computed(() =>
  headSha.value.trim() === '' || COMMIT_SHA.test(headSha.value.trim())
    ? null
    : '40 桁の commit SHA を入力してください（git rev-parse HEAD）。',
);
/** 入力欄に書いたまま「下書きに追加」されていない指摘（確定しても送られない） */
const hasPendingInput = computed(
  () => draft.value.title.trim() !== '' || draft.value.body.trim() !== '',
);
/** 捨てると失われる下書き（積んだ指摘、入力中の指摘、総評） */
const hasDraft = computed(
  () =>
    staged.value.length > 0 ||
    summary.value.trim() !== '' ||
    hasPendingInput.value ||
    draft.value.file.trim() !== '' ||
    draft.value.line.trim() !== '',
);
watch(hasDraft, (dirty) => emit('dirty-change', dirty));

const pendingInputMessage =
  '入力欄の指摘がまだ下書きに追加されていません。「下書きに追加」で積むか、入力を消してから確定してください。';
const canCommit = computed(() => COMMIT_SHA.test(headSha.value.trim()) && !hasPendingInput.value);

const confirmingCancel = ref(false);
function requestCancel() {
  if (hasDraft.value) {
    confirmingCancel.value = true;
    return;
  }
  emit('cancel');
}
function confirmCancel() {
  confirmingCancel.value = false;
  emit('cancel');
}

function addToDraft() {
  draftError.value = null;
  if (draft.value.title.trim() === '' || draft.value.body.trim() === '') {
    draftError.value = 'タイトルと本文を入力してください。';
    return;
  }
  if (draft.value.line.trim() !== '' && !/^\d+$/.test(draft.value.line.trim())) {
    draftError.value = '行番号は数値で入力してください。';
    return;
  }
  staged.value = [...staged.value, { ...draft.value }];
  draft.value = emptyDraft();
}

function removeStaged(index: number) {
  staged.value = staged.value.filter((_, i) => i !== index);
}

async function commit() {
  if (!canCommit.value) return;
  submitError.value = null;
  try {
    await createReview.mutateAsync({
      params: { path: { tenant_id: props.tenantId, project_id: props.projectId } },
      body: {
        pr_number: props.prNumber,
        head_sha: headSha.value.trim(),
        summary: summary.value,
        findings: staged.value.map((finding) => ({
          severity: finding.severity,
          title: finding.title.trim(),
          body: finding.body,
          file: finding.file.trim() === '' ? null : finding.file.trim(),
          line: finding.line.trim() === '' ? null : Number(finding.line.trim()),
        })),
      },
    });
    emit('created');
  } catch (e) {
    const status = (e as { response?: { status?: number } }).response?.status;
    submitError.value =
      status === 403
        ? 'このプロジェクトにレビューを起票する権限がありません。'
        : status === 400
          ? '入力内容を確認してください。'
          : 'ラウンドを確定できませんでした。';
  }
}
</script>

<template>
  <div class="bg-card flex flex-col gap-4 rounded-lg border p-4" data-testid="round-composer">
    <div class="flex flex-col gap-0.5">
      <h2 class="text-base font-semibold">新しいラウンド（R{{ nextRound }}）を起票</h2>
      <p class="text-muted-foreground text-xs">下書き · 確定するまでサーバーには作られません</p>
    </div>

    <div class="grid gap-4 md:grid-cols-2">
      <Field
        id="composer-head"
        label="レビュー対象の head SHA"
        description="40 桁の commit SHA（git rev-parse HEAD）"
        required
        required-mark="asterisk"
        description-position="bottom"
        :error="headShaError ?? ''"
      >
        <Input
          placeholder="60cdd7795f94fa4e4148ce996c2efb4c363e3f5e"
          class="[&_input]:font-mono"
          :value="headSha"
          :on-value-change="(v: string) => (headSha = v)"
        />
      </Field>

      <Field id="composer-summary" label="総評">
        <Textarea
          :rows="2"
          placeholder="具体的な不具合は見つからなかった"
          :value="summary"
          :on-value-change="(v: string) => (summary = v)"
        />
      </Field>
    </div>

    <!-- 下書きに積んだ指摘 -->
    <div v-if="staged.length > 0" class="flex flex-col gap-2">
      <p class="text-sm font-medium">下書きの指摘 {{ staged.length }} 件</p>
      <ul class="flex flex-col gap-1" data-testid="staged-list">
        <li
          v-for="(finding, index) in staged"
          :key="`${finding.title}-${index}`"
          class="bg-muted flex items-center gap-2 rounded-md py-1 pr-1 pl-3"
        >
          <span
            class="flex shrink-0 items-center gap-1.5 text-xs"
            :class="SEVERITY_CLASS[finding.severity]"
          >
            <span class="size-1.5 rounded-full bg-current" aria-hidden="true" />
            {{ SEVERITY_LABELS[finding.severity] }}
          </span>
          <span class="min-w-0 flex-1 truncate text-sm">{{ finding.title }}</span>
          <span v-if="finding.file" class="text-muted-foreground truncate font-mono text-xs">
            {{ finding.file }}{{ finding.line ? `:${finding.line}` : '' }}
          </span>
          <Button
            type="button"
            variant="ghost"
            :ariaLabel="`下書きから「${finding.title}」を外す`"
            @click="removeStaged(index)"
          >
            <template #icon><PhX class="size-4" /></template>
          </Button>
        </li>
      </ul>
    </div>

    <!-- 指摘の入力 -->
    <div class="flex flex-col gap-3 rounded-md bg-(--koyori-color-surface-hover) p-3">
      <div class="flex flex-wrap gap-2">
        <div class="w-[130px]">
          <Field id="draft-severity" label="重大度">
            <Picker
              label="重大度"
              :searchable="false"
              :items="severityItems"
              :selected-values="[draft.severity]"
              :on-selection-change="
                (values: string[]) => (draft.severity = values[0] as FindingSeverity)
              "
            />
          </Field>
        </div>
        <div class="min-w-[200px] flex-1">
          <Field id="draft-title" label="タイトル" required required-mark="asterisk">
            <Input
              placeholder="1 行の要約"
              :value="draft.title"
              :on-value-change="(v: string) => (draft.title = v)"
            />
          </Field>
        </div>
        <div class="w-full sm:w-[220px]">
          <Field id="draft-file" label="ファイル">
            <Input
              placeholder="src/App.vue"
              class="[&_input]:font-mono"
              :value="draft.file"
              :on-value-change="(v: string) => (draft.file = v)"
            />
          </Field>
        </div>
        <div class="w-[80px]">
          <Field id="draft-line" label="行">
            <Input
              input-mode="numeric"
              placeholder="42"
              :value="draft.line"
              :on-value-change="(v: string) => (draft.line = v)"
            />
          </Field>
        </div>
      </div>

      <Field id="draft-body" label="本文">
        <Textarea
          :rows="3"
          placeholder="再現条件と根拠"
          :value="draft.body"
          :on-value-change="(v: string) => (draft.body = v)"
        />
      </Field>

      <p v-if="draftError" role="alert" class="text-destructive text-sm">{{ draftError }}</p>

      <div>
        <Button type="button" variant="tertiary" label="下書きに追加" @click="addToDraft">
          <template #icon><PhPlus class="size-4" /></template>
        </Button>
      </div>
    </div>

    <p class="text-muted-foreground text-xs">
      {{
        staged.length === 0
          ? '指摘 0 件でも確定できます（総評だけのラウンドとして記録）'
          : `確定すると ${staged.length} 件が Open として一括作成され、R${nextRound} は以後追記できません`
      }}
    </p>

    <p v-if="hasPendingInput" role="alert" class="text-destructive text-sm">
      {{ pendingInputMessage }}
    </p>

    <p v-if="submitError" role="alert" class="text-destructive text-sm">{{ submitError }}</p>

    <div class="flex gap-2">
      <Button
        type="button"
        :disabled="!canCommit || createReview.isPending.value"
        :title="
          hasPendingInput
            ? pendingInputMessage
            : canCommit
              ? undefined
              : '40 桁の commit SHA を入力してください'
        "
        :label="createReview.isPending.value ? '確定中…' : `確定（${staged.length} 件）`"
        @click="commit"
      />
      <Button type="button" variant="ghost" label="キャンセル" @click="requestCancel" />
    </div>

    <ConfirmDialog
      :open="confirmingCancel"
      title="下書きを破棄しますか？"
      message="入力中の指摘と下書きに積んだ指摘は、サーバーに作られていないため失われます。"
      confirm-label="破棄する"
      destructive
      :on-confirm="confirmCancel"
      :on-cancel="() => (confirmingCancel = false)"
    />
  </div>
</template>
