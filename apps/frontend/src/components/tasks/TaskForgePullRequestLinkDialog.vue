<script setup lang="ts">
import { useForm } from '@tanstack/vue-form';
import { useQueryClient } from '@tanstack/vue-query';
import { OpenApiVueQueryError } from '@koyori-app/openapi-vue-query';
import { type } from 'arktype';
import { ref } from 'vue';

import { Button, Field, Input } from '@koyori-app/ui-vue';
import { Checkbox } from '@/components/ui/checkbox';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { ACTIVITIES_PATH } from '@/composables/useTaskActivities';
import { apiClient } from '@/lib/api-vue-query';

/**
 * タスクに PR を手でリンクするダイアログ（docs/features/tasks/9.github-tasks.md §8 / §9）。
 * リポジトリはプロジェクトの連携先で決まるので、表示するだけで選ばせない。
 * 開くたびに `v-if` でマウントし直すこと（フォームの初期化のため）。
 */
const LINK_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/forge/pull-requests' as const;
const FORGE_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/forge' as const;

const props = defineProps<{
  tenantId: string;
  projectId: string;
  taskId: string;
  /** `owner/repo`。連携先を読めない（権限が無い等）ときは null */
  repoFullName: string | null;
}>();

const emit = defineEmits<{ close: [] }>();

const queryClient = useQueryClient();
const submitError = ref<string | null>(null);
const linkMutation = apiClient.useMutation('post', LINK_PATH);

const schema = type({
  number: /^\s*[1-9]\d{0,8}\s*$/,
  closes: 'boolean',
});

const form = useForm({
  defaultValues: { number: '', closes: false },
  validators: { onSubmit: schema },
  onSubmit: async ({ value }) => {
    submitError.value = null;
    try {
      await linkMutation.mutateAsync({
        params: {
          path: { tenant_id: props.tenantId, project_id: props.projectId, id: props.taskId },
        },
        body: { number: Number(value.number.trim()), closes: value.closes },
      });
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['get', FORGE_PATH] }),
        queryClient.invalidateQueries({ queryKey: ['get', ACTIVITIES_PATH] }),
      ]);
      emit('close');
    } catch (error) {
      submitError.value =
        error instanceof OpenApiVueQueryError && error.response?.status === 409
          ? 'このプロジェクトは GitHub と連携していません'
          : 'PR をリンクできませんでした';
    }
  },
});

function onOpenChange(open: boolean) {
  // 送信中は閉じない（結果の見逃し防止）
  if (!open && linkMutation.isPending.value) return;
  if (!open) emit('close');
}
</script>

<template>
  <Dialog :open="true" @update:open="onOpenChange">
    <DialogContent class="max-w-md">
      <DialogHeader>
        <DialogTitle>PR をリンク</DialogTitle>
        <DialogDescription>
          {{
            repoFullName
              ? `${repoFullName} の PR をこのタスクに結びます。`
              : 'プロジェクトに連携したリポジトリの PR をこのタスクに結びます。'
          }}
        </DialogDescription>
      </DialogHeader>

      <form class="flex flex-col gap-4" @submit.prevent="form.handleSubmit">
        <form.Field name="number">
          <template #default="{ field }">
            <Field
              id="forge-pr-number"
              label="PR 番号"
              :error="field.state.meta.errors.length ? 'PR 番号を数字で入力してください' : ''"
            >
              <Input
                input-mode="numeric"
                placeholder="87"
                autocomplete="off"
                :value="field.state.value"
                :on-value-change="field.handleChange"
                :on-blur="field.handleBlur"
              />
            </Field>
          </template>
        </form.Field>

        <form.Field name="closes">
          <template #default="{ field }">
            <label class="flex cursor-pointer items-center gap-2 text-sm">
              <Checkbox
                :model-value="field.state.value"
                @update:model-value="(v) => field.handleChange(v === true)"
              />
              マージしたらこのタスクを完了にする
            </label>
          </template>
        </form.Field>

        <p v-if="submitError" role="alert" class="text-sm text-destructive">{{ submitError }}</p>

        <DialogFooter>
          <Button
            type="button"
            variant="tertiary"
            :disabled="linkMutation.isPending.value"
            label="キャンセル"
            @click="emit('close')"
          />
          <form.Subscribe>
            <template #default="{ canSubmit, isSubmitting }">
              <Button
                type="submit"
                :disabled="!canSubmit || isSubmitting || linkMutation.isPending.value"
                :label="isSubmitting || linkMutation.isPending.value ? 'リンク中…' : 'リンクする'"
              />
            </template>
          </form.Subscribe>
        </DialogFooter>
      </form>
    </DialogContent>
  </Dialog>
</template>
