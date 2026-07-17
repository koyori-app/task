<script setup lang="ts">
import { useForm } from '@tanstack/vue-form';
import { type } from 'arktype';
import { useQueryClient } from '@tanstack/vue-query';
import {
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogOverlay,
  DialogPortal,
  DialogRoot,
  DialogTitle,
} from 'reka-ui';
import { computed, ref, watch } from 'vue';
import { Button } from '@/components/ui/button';
import { Field, FieldError, FieldGroup, FieldLabel } from '@/components/ui/field';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import { apiClient } from '@/lib/api-vue-query';
import type { components } from '@/generated/api';

type ProjectResponse = components['schemas']['ProjectResponse'];

const LIST_PROJECTS_PATH = '/v1/tenants/{tenant_id}/projects' as const;
const PROJECT_PATH = '/v1/tenants/{tenant_id}/projects/{id}' as const;

const props = defineProps<{
  open: boolean;
  tenantId: string;
  /** 渡すと編集モード。未指定なら作成モード */
  project?: ProjectResponse | null;
}>();

const emit = defineEmits<{
  'update:open': [open: boolean];
  saved: [project: ProjectResponse];
}>();

const queryClient = useQueryClient();
const submitError = ref<string | null>(null);
const keyEdited = ref(false);

const isEdit = computed(() => !!props.project);

/** backend の validate_project_key と同じ制約（空は自動生成に委ねる） */
const PROJECT_KEY_PATTERN = /^$|^[A-Z][A-Z0-9]{1,9}$/;

/** 名前からキー候補を作る（backend の generate_project_key 相当の簡易版） */
function suggestKey(name: string): string {
  const upper = name
    .normalize('NFKD')
    .toUpperCase()
    .replace(/[^A-Z0-9]+/g, '')
    .replace(/^[0-9]+/, '')
    .slice(0, 10);
  return upper.length >= 2 ? upper : '';
}

const nonBlankName = type('string').narrow((name) => name.trim().length >= 1);

const schema = type({
  name: nonBlankName,
  key: PROJECT_KEY_PATTERN,
  description: 'string',
});

const createMutation = apiClient.useMutation('post', LIST_PROJECTS_PATH);
const updateMutation = apiClient.useMutation('put', PROJECT_PATH);

const form = useForm({
  defaultValues: { name: '', key: '', description: '' },
  validators: { onSubmit: schema },
  onSubmit: async ({ value }) => {
    submitError.value = null;
    try {
      let saved: ProjectResponse;
      if (isEdit.value && props.project) {
        saved = await updateMutation.mutateAsync({
          params: { path: { tenant_id: props.tenantId, id: props.project.id } },
          body: {
            name: value.name.trim(),
            description: value.description.trim(),
          },
        });
      } else {
        saved = await createMutation.mutateAsync({
          params: { path: { tenant_id: props.tenantId } },
          body: {
            name: value.name.trim(),
            ...(value.key ? { key: value.key } : {}),
            ...(value.description.trim() ? { description: value.description.trim() } : {}),
          },
        });
      }
      await queryClient.invalidateQueries({ queryKey: ['get', LIST_PROJECTS_PATH] });
      emit('update:open', false);
      emit('saved', saved);
    } catch {
      submitError.value = isEdit.value
        ? 'プロジェクトを更新できませんでした'
        : 'プロジェクトを作成できませんでした';
    }
  },
});

watch(
  () => props.open,
  (open) => {
    submitError.value = null;
    keyEdited.value = false;
    form.reset();
    if (open && props.project) {
      // key は編集不可（表示は props から直接参照）。form state に入れると
      // レガシーな小文字 key がスキーマ検証で落ちて保存できなくなる
      form.setFieldValue('name', props.project.name);
      form.setFieldValue('description', props.project.description);
    }
  },
  // マウント時点で open=true の場合（stories 等）もプリフィルする
  { immediate: true },
);

const isPending = computed(() => createMutation.isPending.value || updateMutation.isPending.value);

function onOpenChange(open: boolean) {
  if (!open && isPending.value) return;
  emit('update:open', open);
}
</script>

<template>
  <DialogRoot :open="open" @update:open="onOpenChange">
    <DialogPortal>
      <DialogOverlay
        class="fixed inset-0 z-50 bg-black/50"
        data-testid="project-form-dialog-overlay"
      />
      <DialogContent
        class="fixed left-1/2 top-1/2 z-50 grid w-[calc(100%-2rem)] max-w-lg -translate-x-1/2 -translate-y-1/2 gap-4 rounded-lg border bg-background p-6 shadow-lg"
      >
        <div class="space-y-1.5">
          <DialogTitle class="text-lg font-semibold">
            {{ isEdit ? 'プロジェクトを編集' : 'プロジェクトを作成' }}
          </DialogTitle>
          <DialogDescription class="text-sm text-muted-foreground">
            {{
              isEdit
                ? 'プロジェクトの名前と説明を変更できます。'
                : '新しいプロジェクトの名前を入力してください。'
            }}
          </DialogDescription>
        </div>
        <form class="space-y-4" @submit.prevent="form.handleSubmit">
          <FieldGroup>
            <form.Field name="name">
              <template #default="{ field }">
                <Field>
                  <FieldLabel :for="field.name">名前</FieldLabel>
                  <Input
                    :id="field.name"
                    autofocus
                    :model-value="field.state.value"
                    @blur="field.handleBlur"
                    @update:model-value="
                      (v) => {
                        const name = String(v);
                        field.handleChange(name);
                        if (!isEdit && !keyEdited) form.setFieldValue('key', suggestKey(name));
                      }
                    "
                  />
                  <FieldError v-if="field.state.meta.isTouched && field.state.meta.errors.length"
                    >名前は必須です</FieldError
                  >
                </Field>
              </template>
            </form.Field>
            <form.Field v-if="!isEdit" name="key">
              <template #default="{ field }">
                <Field>
                  <FieldLabel :for="field.name">キー（任意）</FieldLabel>
                  <Input
                    :id="field.name"
                    :model-value="field.state.value"
                    aria-describedby="project-key-help"
                    @blur="field.handleBlur"
                    @update:model-value="
                      (v) => {
                        keyEdited = true;
                        field.handleChange(String(v).toUpperCase());
                      }
                    "
                  />
                  <p id="project-key-help" class="text-xs text-muted-foreground">
                    2〜10 文字・先頭は大文字英字（例: ENG）。空欄なら自動生成
                  </p>
                  <FieldError v-if="field.state.meta.isTouched && field.state.meta.errors.length"
                    >キーは 2〜10
                    文字で、先頭は大文字英字、残りは大文字英字または数字です</FieldError
                  >
                </Field>
              </template>
            </form.Field>
            <Field v-else>
              <FieldLabel for="project-key-readonly">キー</FieldLabel>
              <Input
                id="project-key-readonly"
                :model-value="props.project?.key ?? ''"
                disabled
                aria-describedby="project-key-fixed-help"
              />
              <p id="project-key-fixed-help" class="text-xs text-muted-foreground">
                キーはタスク番号（例: ENG-1）の基準のため変更できません
              </p>
            </Field>
            <form.Field name="description">
              <template #default="{ field }">
                <Field>
                  <FieldLabel :for="field.name">説明（任意）</FieldLabel>
                  <Textarea
                    :id="field.name"
                    :model-value="field.state.value"
                    @update:model-value="(v) => field.handleChange(String(v))"
                  />
                </Field>
              </template>
            </form.Field>
          </FieldGroup>
          <p v-if="submitError" role="alert" class="text-sm text-destructive">{{ submitError }}</p>
          <div class="flex justify-end gap-2">
            <DialogClose as-child>
              <Button type="button" variant="outline" :disabled="isPending">キャンセル</Button>
            </DialogClose>
            <form.Subscribe>
              <template #default="{ canSubmit, isSubmitting }">
                <Button type="submit" :disabled="!canSubmit || isSubmitting">
                  {{ isSubmitting ? (isEdit ? '保存中…' : '作成中…') : isEdit ? '保存' : '作成' }}
                </Button>
              </template>
            </form.Subscribe>
          </div>
        </form>
      </DialogContent>
    </DialogPortal>
  </DialogRoot>
</template>
