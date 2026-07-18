<script setup lang="ts">
import { useForm } from '@tanstack/vue-form';
import { type } from 'arktype';
import { useQueryClient } from '@tanstack/vue-query';
import { PhPencilSimple, PhPlus, PhTrash } from '@phosphor-icons/vue';
import { computed, ref } from 'vue';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Field, FieldError, FieldLabel } from '@/components/ui/field';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Textarea } from '@/components/ui/textarea';
import {
  CUSTOM_FIELD_TYPES,
  customFieldTypeMeta,
  parseSelectOptions,
  type CustomFieldType,
} from '@/components/projects/custom-field-types';
import { apiClient } from '@/lib/api-vue-query';
import type { components } from '@/generated/api';

type CustomFieldResponse = components['schemas']['ProjectCustomFieldResponse'];

const FIELDS_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/custom-fields' as const;
const FIELD_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/custom-fields/{field_id}' as const;

const props = defineProps<{
  tenantId: string;
  projectId: string;
}>();

const queryClient = useQueryClient();

const listQuery = apiClient.useQuery('get', FIELDS_PATH, {
  params: { path: { tenant_id: props.tenantId, project_id: props.projectId } },
});
const fields = computed(() => listQuery.data.value?.fields ?? []);

const createMutation = apiClient.useMutation('post', FIELDS_PATH);
const updateMutation = apiClient.useMutation('patch', FIELD_PATH);
const deleteMutation = apiClient.useMutation('delete', FIELD_PATH);

const isFormOpen = ref(false);
const editingField = ref<CustomFieldResponse | null>(null);
const formError = ref<string | null>(null);
const deleteTarget = ref<CustomFieldResponse | null>(null);
const deleteError = ref<string | null>(null);

// backend の CreateCustomFieldRequest / UpdateCustomFieldRequest（length(min=1, max=100)）と同じ制約
const nonBlankName = type('string').narrow((name) => {
  const length = name.trim().length;
  return length >= 1 && length <= 100;
});

const schema = type({
  name: nonBlankName,
  fieldType: "'text'|'number'|'select'|'date'|'url'|'checkbox'",
  optionsText: 'string',
});

const form = useForm({
  defaultValues: { name: '', fieldType: 'text' as CustomFieldType, optionsText: '' },
  validators: { onSubmit: schema },
  onSubmit: async ({ value }) => {
    formError.value = null;
    const target = editingField.value;
    try {
      if (target) {
        // field_type は PATCH で変更できないため名前のみ更新する
        await updateMutation.mutateAsync({
          params: {
            path: { tenant_id: props.tenantId, project_id: props.projectId, field_id: target.id },
          },
          body: { name: value.name.trim() },
        });
      } else {
        const options = parseSelectOptions(value.optionsText);
        if (value.fieldType === 'select' && options.length === 0) {
          formError.value = '選択肢を1行に1つ以上入力してください';
          return;
        }
        await createMutation.mutateAsync({
          params: { path: { tenant_id: props.tenantId, project_id: props.projectId } },
          body: {
            name: value.name.trim(),
            field_type: value.fieldType,
            // select 以外に options を送ると backend が 400 を返す
            ...(value.fieldType === 'select' ? { options } : {}),
          },
        });
      }
      await queryClient.invalidateQueries({ queryKey: ['get', FIELDS_PATH] });
      isFormOpen.value = false;
    } catch {
      formError.value = target
        ? 'カスタムフィールドを更新できませんでした'
        : 'カスタムフィールドを追加できませんでした';
    }
  },
});

const formFieldType = form.useStore((state) => state.values.fieldType);

const isSaving = computed(() => createMutation.isPending.value || updateMutation.isPending.value);

const submitLabel = computed(() => {
  if (isSaving.value) return editingField.value ? '保存中…' : '追加中…';
  return editingField.value ? '保存する' : '追加する';
});

function openAdd() {
  editingField.value = null;
  formError.value = null;
  form.reset();
  isFormOpen.value = true;
}

function openEdit(field: CustomFieldResponse) {
  editingField.value = field;
  formError.value = null;
  form.reset();
  form.setFieldValue('name', field.name);
  form.setFieldValue('fieldType', field.field_type);
  isFormOpen.value = true;
}

function onFormOpenChange(open: boolean) {
  // 保存リクエスト進行中はダイアログを閉じない（結果の見逃し防止）
  if (!open && isSaving.value) return;
  isFormOpen.value = open;
}

function openDelete(field: CustomFieldResponse) {
  deleteError.value = null;
  deleteTarget.value = field;
}

function onDeleteOpenChange(open: boolean) {
  if (!open && deleteMutation.isPending.value) return;
  if (!open) deleteTarget.value = null;
}

async function confirmDelete() {
  const target = deleteTarget.value;
  if (!target) return;
  deleteError.value = null;
  try {
    await deleteMutation.mutateAsync({
      params: {
        path: { tenant_id: props.tenantId, project_id: props.projectId, field_id: target.id },
      },
    });
    await queryClient.invalidateQueries({ queryKey: ['get', FIELDS_PATH] });
    deleteTarget.value = null;
  } catch {
    deleteError.value = 'カスタムフィールドを削除できませんでした';
  }
}
</script>

<template>
  <div>
    <div class="mb-5 flex items-center justify-between gap-4 border-b pb-4">
      <h2 class="text-xl font-semibold">カスタムフィールド</h2>
      <Button type="button" variant="outline" class="shrink-0" @click="openAdd">
        <PhPlus class="size-4" />
        フィールドを追加
      </Button>
    </div>
    <p class="mb-4 text-sm text-muted-foreground">
      このプロジェクトのすべてのタスクに表示される追加の属性です。
    </p>

    <p v-if="listQuery.isPending.value" class="text-sm text-muted-foreground">読み込み中…</p>
    <p v-else-if="listQuery.isError.value" role="alert" class="text-sm text-destructive">
      カスタムフィールドを読み込めませんでした
    </p>
    <p v-else-if="fields.length === 0" class="text-sm text-muted-foreground">
      カスタムフィールドはまだありません
    </p>
    <ul v-else class="overflow-hidden rounded-lg border" aria-label="カスタムフィールド一覧">
      <li
        v-for="field in fields"
        :key="field.id"
        class="flex items-center gap-3 border-b px-3.5 py-3 last:border-b-0"
      >
        <component
          :is="customFieldTypeMeta(field.field_type).icon"
          class="size-4 shrink-0 text-muted-foreground"
          aria-hidden="true"
        />
        <span class="min-w-0 flex-1 truncate text-sm font-medium">{{ field.name }}</span>
        <span
          class="shrink-0 rounded-full border px-2.5 py-0.5 text-[11px] font-medium text-muted-foreground"
        >
          {{ customFieldTypeMeta(field.field_type).label }}
        </span>
        <button
          type="button"
          class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground"
          :aria-label="`${field.name} を編集`"
          @click="openEdit(field)"
        >
          <PhPencilSimple class="size-4" />
        </button>
        <button
          type="button"
          class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-destructive"
          :aria-label="`${field.name} を削除`"
          @click="openDelete(field)"
        >
          <PhTrash class="size-4" />
        </button>
      </li>
    </ul>

    <!-- 追加・編集ダイアログ -->
    <Dialog v-if="isFormOpen" :open="true" @update:open="onFormOpenChange">
      <DialogContent class="max-w-md">
        <DialogHeader>
          <DialogTitle>{{ editingField ? 'フィールドを編集' : 'フィールドを追加' }}</DialogTitle>
          <DialogDescription>
            {{
              editingField
                ? '名前を変更できます。型は変更できません。'
                : 'タスクに表示されるフィールドの名前と型を設定します。'
            }}
          </DialogDescription>
        </DialogHeader>
        <form class="flex flex-col gap-4" @submit.prevent="form.handleSubmit">
          <form.Field name="name">
            <template #default="{ field }">
              <Field>
                <FieldLabel :for="field.name">名前</FieldLabel>
                <Input
                  :id="field.name"
                  placeholder="例: 見積もり"
                  maxlength="100"
                  :model-value="field.state.value"
                  @blur="field.handleBlur"
                  @update:model-value="(v) => field.handleChange(String(v))"
                />
                <FieldError v-if="field.state.meta.isTouched && field.state.meta.errors.length"
                  >名前は 1〜100 文字で入力してください</FieldError
                >
              </Field>
            </template>
          </form.Field>

          <template v-if="!editingField">
            <form.Field name="fieldType">
              <template #default="{ field }">
                <Field>
                  <FieldLabel for="custom-field-type">型</FieldLabel>
                  <Select
                    :model-value="field.state.value"
                    @update:model-value="(v) => field.handleChange(v as CustomFieldType)"
                  >
                    <SelectTrigger id="custom-field-type" class="w-full">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem v-for="t in CUSTOM_FIELD_TYPES" :key="t.value" :value="t.value">
                        {{ t.label }}
                      </SelectItem>
                    </SelectContent>
                  </Select>
                </Field>
              </template>
            </form.Field>

            <form.Field v-if="formFieldType === 'select'" name="optionsText">
              <template #default="{ field }">
                <Field>
                  <FieldLabel :for="field.name">選択肢（1行に1つ）</FieldLabel>
                  <Textarea
                    :id="field.name"
                    rows="4"
                    placeholder="高&#10;中&#10;低"
                    :model-value="field.state.value"
                    @update:model-value="(v) => field.handleChange(String(v))"
                  />
                </Field>
              </template>
            </form.Field>
          </template>

          <p v-if="formError" role="alert" class="text-sm text-destructive">{{ formError }}</p>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              :disabled="isSaving"
              @click="onFormOpenChange(false)"
            >
              キャンセル
            </Button>
            <form.Subscribe>
              <template #default="{ canSubmit, isSubmitting }">
                <Button type="submit" :disabled="!canSubmit || isSubmitting || isSaving">
                  {{ submitLabel }}
                </Button>
              </template>
            </form.Subscribe>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>

    <!-- 削除確認ダイアログ -->
    <Dialog v-if="deleteTarget" :open="true" @update:open="onDeleteOpenChange">
      <DialogContent class="max-w-md" :show-close-button="false">
        <DialogHeader>
          <DialogTitle>カスタムフィールドを削除しますか？</DialogTitle>
          <DialogDescription>
            「{{
              deleteTarget?.name
            }}」を削除します。タスクに設定された値も失われます。この操作は取り消せません。
          </DialogDescription>
        </DialogHeader>
        <p v-if="deleteError" role="alert" class="text-sm text-destructive">{{ deleteError }}</p>
        <DialogFooter>
          <Button
            type="button"
            variant="outline"
            :disabled="deleteMutation.isPending.value"
            @click="onDeleteOpenChange(false)"
          >
            キャンセル
          </Button>
          <Button
            type="button"
            variant="destructive"
            :disabled="deleteMutation.isPending.value"
            @click="confirmDelete"
          >
            {{ deleteMutation.isPending.value ? '削除中…' : '削除する' }}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </div>
</template>
