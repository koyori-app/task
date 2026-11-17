<script setup lang="ts">
import { useForm } from '@tanstack/vue-form';
import { type } from 'arktype';
import { useQueryClient } from '@tanstack/vue-query';
import { computed, ref, useId } from 'vue';
import { Button, Dialog, Field, Input } from '@koyori-app/ui-vue';
import { apiClient } from '@/lib/api-vue-query';
import { invalidateTaskCachesAfterTaskTypeChange } from '@/lib/task-type-cache';
import type { components } from '@/generated/api';

type TaskTypeResponse = components['schemas']['TaskTypeResponse'];

const TASK_TYPES_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/task-types' as const;
const TASK_TYPE_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/task-types/{id}' as const;

/** 種別の色のプリセット（自由入力も可） */
const TASK_TYPE_COLOR_PRESETS = [
  '#ef4444',
  '#f97316',
  '#f59e0b',
  '#22c55e',
  '#06b6d4',
  '#3b82f6',
  '#8b5cf6',
  '#ec4899',
  '#64748b',
] as const;

const COLOR_PATTERN = /^#[0-9a-fA-F]{6}$/;

/**
 * 種別の作成・編集ダイアログ。`taskType` が null なら作成、あれば編集。
 * フォーム初期値を props から取るため、親は開くたびに `v-if` でマウントし直すこと。
 */
const props = defineProps<{
  tenantId: string;
  projectId: string;
  taskType: TaskTypeResponse | null;
}>();

const emit = defineEmits<{ close: [] }>();
// 閉じた Dialog も DOM に残るため、送信ボタンを結ぶ form の id はインスタンスごとに分ける
const formId = useId();

const queryClient = useQueryClient();
const submitError = ref<string | null>(null);

// backend の CreateTaskTypeRequest / UpdateTaskTypeRequest（length(min=1, max=100)）と同じ制約。
// validator crate は chars().count()（コードポイント単位）で数えるため、UTF-16 単位の
// String.length / maxlength ではなく Array.from で数える（絵文字を 2 と数えない）
const nonBlankName = type('string').narrow((name) => {
  const length = Array.from(name.trim()).length;
  return length >= 1 && length <= 100;
});

const schema = type({
  name: nonBlankName,
  color: COLOR_PATTERN,
});

const createMutation = apiClient.useMutation('post', TASK_TYPES_PATH);
const updateMutation = apiClient.useMutation('put', TASK_TYPE_PATH);

const isPending = computed(() => createMutation.isPending.value || updateMutation.isPending.value);

const form = useForm({
  defaultValues: {
    name: props.taskType?.name ?? '',
    color: props.taskType?.color ?? TASK_TYPE_COLOR_PRESETS[0],
  },
  validators: { onSubmit: schema },
  onSubmit: async ({ value }) => {
    submitError.value = null;
    const body = { name: value.name.trim(), color: value.color };
    try {
      if (props.taskType) {
        await updateMutation.mutateAsync({
          params: {
            path: { tenant_id: props.tenantId, project_id: props.projectId, id: props.taskType.id },
          },
          body,
        });
      } else {
        await createMutation.mutateAsync({
          params: { path: { tenant_id: props.tenantId, project_id: props.projectId } },
          body,
        });
      }
      await queryClient.invalidateQueries({ queryKey: ['get', TASK_TYPES_PATH] });
      // 名前と色を変えたら、タスクに埋め込まれた種別も古くなる
      if (props.taskType) await invalidateTaskCachesAfterTaskTypeChange(queryClient);
      emit('close');
    } catch {
      submitError.value = '種別を保存できませんでした';
    }
  },
});

function onClose() {
  // 保存リクエスト進行中はダイアログを閉じない（結果の見逃し防止）
  if (isPending.value) return;
  emit('close');
}
</script>

<template>
  <Dialog
    :open="true"
    :title="taskType ? '種別を編集' : '種別を作成'"
    description="タスクの種類を表す名前と色を設定します。1 つのタスクに種別は 1 つだけ付けられます。"
    :on-close="onClose"
  >
    <form :id="formId" class="flex flex-col gap-4" @submit.prevent="form.handleSubmit">
      <form.Field name="name">
        <template #default="{ field }">
          <Field
            id="task-type-name"
            label="名前"
            :error="field.state.meta.errors.length ? '名前は 1〜100 文字で入力してください' : ''"
          >
            <Input
              placeholder="バグ"
              :value="field.state.value"
              :on-value-change="field.handleChange"
              :on-blur="field.handleBlur"
            />
          </Field>
        </template>
      </form.Field>

      <form.Field name="color">
        <template #default="{ field }">
          <Field
            id="task-type-color"
            label="色"
            :error="field.state.meta.errors.length ? '色は #RRGGBB 形式で入力してください' : ''"
          >
            <div class="flex flex-wrap items-center gap-1.5" role="group" aria-label="色プリセット">
              <button
                v-for="preset in TASK_TYPE_COLOR_PRESETS"
                :key="preset"
                type="button"
                class="size-6 rounded-full border"
                :class="
                  field.state.value.toLowerCase() === preset
                    ? 'ring-2 ring-ring ring-offset-2 ring-offset-background'
                    : ''
                "
                :style="{ backgroundColor: preset }"
                :aria-label="`色 ${preset}`"
                :aria-pressed="field.state.value.toLowerCase() === preset"
                @click="field.handleChange(preset)"
              />
            </div>
            <!-- 文字数は送信時に COLOR_PATTERN で検証する -->
            <Input
              class="max-w-[120px] [&_input]:font-mono"
              :value="field.state.value"
              :on-value-change="field.handleChange"
              :on-blur="field.handleBlur"
            />
          </Field>
        </template>
      </form.Field>

      <p v-if="submitError" role="alert" class="text-sm text-destructive">{{ submitError }}</p>
    </form>

    <template #actions>
      <Button
        type="button"
        variant="tertiary"
        :disabled="isPending"
        label="キャンセル"
        @click="emit('close')"
      />
      <form.Subscribe>
        <template #default="{ canSubmit, isSubmitting }">
          <Button
            type="submit"
            :form="formId"
            :disabled="!canSubmit || isSubmitting || isPending"
            :label="isSubmitting || isPending ? '保存中…' : taskType ? '変更を保存' : '種別を作成'"
          />
        </template>
      </form.Subscribe>
    </template>
  </Dialog>
</template>
