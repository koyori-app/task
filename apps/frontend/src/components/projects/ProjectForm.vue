<script setup lang="ts">
import { useForm } from '@tanstack/vue-form';
import { type } from 'arktype';
import { useQueryClient } from '@tanstack/vue-query';
import { navigate } from 'vike/client/router';
import { computed, ref } from 'vue';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Field, FieldError, FieldLabel } from '@/components/ui/field';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import DeleteProjectDialog from '@/components/sidebar/DeleteProjectDialog.vue';
import { apiClient } from '@/lib/api-vue-query';
import type { components } from '@/generated/api';

type ProjectResponse = components['schemas']['ProjectResponse'];

const LIST_PROJECTS_PATH = '/v1/tenants/{tenant_id}/projects' as const;
const PROJECT_PATH = '/v1/tenants/{tenant_id}/projects/{id}' as const;

/** デザイン準拠の選択肢（先頭は「なし」相当のクリア） */
const EMOJI_CHOICES = ['📁', '⚙️', '🎨', '🚀', '📊', '🧪', '📦', '🔧', '🌏', '💡', '🧭', '🗂️'];

const props = defineProps<{
  tenantId: string;
  tenantSlug: string;
  /** 渡すと設定（編集）モード。未指定なら作成モード */
  project?: ProjectResponse | null;
}>();

const queryClient = useQueryClient();
const submitError = ref<string | null>(null);
const keyEdited = ref(false);
const isDeleteOpen = ref(false);

const isEdit = computed(() => !!props.project);
const icon = ref<string | null>(props.project?.icon_emoji ?? null);

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
  defaultValues: {
    name: props.project?.name ?? '',
    key: '',
    description: props.project?.description ?? '',
  },
  validators: { onSubmit: schema },
  onSubmit: async ({ value }) => {
    submitError.value = null;
    try {
      let saved: ProjectResponse;
      if (isEdit.value && props.project) {
        const originalIcon = props.project.icon_emoji ?? null;
        saved = await updateMutation.mutateAsync({
          params: { path: { tenant_id: props.tenantId, id: props.project.id } },
          body: {
            name: value.name.trim(),
            description: value.description.trim(),
            ...(icon.value && icon.value !== originalIcon ? { icon_emoji: icon.value } : {}),
            ...(!icon.value && originalIcon ? { clear_icon_emoji: true } : {}),
          },
        });
      } else {
        saved = await createMutation.mutateAsync({
          params: { path: { tenant_id: props.tenantId } },
          body: {
            name: value.name.trim(),
            ...(value.key ? { key: value.key } : {}),
            ...(value.description.trim() ? { description: value.description.trim() } : {}),
            ...(icon.value ? { icon_emoji: icon.value } : {}),
          },
        });
      }
      await queryClient.invalidateQueries({ queryKey: ['get', LIST_PROJECTS_PATH] });
      void navigate(`/${props.tenantSlug}/projects/${saved.key}/tasks`);
    } catch {
      submitError.value = isEdit.value
        ? 'プロジェクトを更新できませんでした'
        : 'プロジェクトを作成できませんでした';
    }
  },
});

const isPending = computed(() => createMutation.isPending.value || updateMutation.isPending.value);

const cancelHref = computed(() =>
  isEdit.value && props.project
    ? `/${props.tenantSlug}/projects/${props.project.key}/tasks`
    : `/${props.tenantSlug}/my-tasks`,
);

function onCancel() {
  void navigate(cancelHref.value);
}

function onDeleted() {
  void navigate(`/${props.tenantSlug}/my-tasks`);
}
</script>

<template>
  <div class="mx-auto w-full max-w-[720px]">
    <div class="mb-6">
      <h1 class="mb-1 text-3xl font-bold tracking-tight">
        {{ isEdit ? 'プロジェクト設定' : 'プロジェクトを作成' }}
      </h1>
      <p class="text-sm text-muted-foreground">
        {{
          isEdit
            ? 'このプロジェクトの詳細を管理します。'
            : '新しいタスクのためのワークスペースを用意します。'
        }}
      </p>
    </div>

    <form @submit.prevent="form.handleSubmit">
      <!-- Details カード -->
      <section class="mb-4 rounded-[10px] border bg-card p-6">
        <h2 class="mb-1 text-base font-semibold">詳細</h2>
        <p class="mb-5 text-sm text-muted-foreground">
          プロジェクトの名前・キー・アイコンを設定します。
        </p>

        <div class="mb-5 flex items-start gap-4">
          <div class="shrink-0">
            <span class="mb-1.5 block text-sm font-medium">アイコン</span>
            <DropdownMenu>
              <DropdownMenuTrigger as-child>
                <button
                  type="button"
                  aria-label="アイコンを選択"
                  class="flex size-14 items-center justify-center rounded-[10px] border bg-secondary text-[26px] shadow-sm"
                >
                  <span v-if="icon">{{ icon }}</span>
                  <span v-else class="text-sm text-muted-foreground">なし</span>
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="start" class="w-[236px] p-2">
                <div class="grid grid-cols-6 gap-0.5">
                  <button
                    v-for="choice in EMOJI_CHOICES"
                    :key="choice"
                    type="button"
                    class="size-[34px] rounded-md text-lg hover:bg-accent"
                    :aria-label="`アイコン ${choice}`"
                    @click="icon = choice"
                  >
                    {{ choice }}
                  </button>
                </div>
                <button
                  type="button"
                  class="mt-1 w-full rounded-md px-2 py-1.5 text-left text-sm text-muted-foreground hover:bg-accent"
                  @click="icon = null"
                >
                  アイコンなし
                </button>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>

          <div class="min-w-0 flex-1">
            <form.Field name="name">
              <template #default="{ field }">
                <Field>
                  <FieldLabel :for="field.name">名前</FieldLabel>
                  <Input
                    :id="field.name"
                    autofocus
                    placeholder="プロジェクト名"
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
                  <p class="text-xs text-muted-foreground">サイドバーとパンくずに表示されます。</p>
                  <FieldError v-if="field.state.meta.isTouched && field.state.meta.errors.length"
                    >名前は必須です</FieldError
                  >
                </Field>
              </template>
            </form.Field>
          </div>
        </div>

        <div class="mb-5 max-w-[220px]">
          <form.Field v-if="!isEdit" name="key">
            <template #default="{ field }">
              <Field>
                <FieldLabel :for="field.name">キー</FieldLabel>
                <Input
                  :id="field.name"
                  placeholder="ABC"
                  maxlength="10"
                  class="font-mono uppercase"
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
                  タスク番号の接頭辞（例:
                  <span class="font-mono">{{ (field.state.value || 'ABC') + '-1' }}</span
                  >）。空欄なら自動生成
                </p>
                <FieldError v-if="field.state.meta.isTouched && field.state.meta.errors.length"
                  >キーは 2〜10 文字で、先頭は大文字英字、残りは大文字英字または数字です</FieldError
                >
              </Field>
            </template>
          </form.Field>
          <Field v-else>
            <FieldLabel for="project-key-readonly">キー</FieldLabel>
            <Input
              id="project-key-readonly"
              class="font-mono"
              :model-value="props.project?.key ?? ''"
              disabled
              aria-describedby="project-key-fixed-help"
            />
            <p id="project-key-fixed-help" class="text-xs text-muted-foreground">
              タスク番号（例:
              <span class="font-mono">{{ (props.project?.key ?? 'ABC') + '-1' }}</span
              >）の基準のため変更できません
            </p>
          </Field>
        </div>

        <form.Field name="description">
          <template #default="{ field }">
            <Field>
              <FieldLabel :for="field.name">説明</FieldLabel>
              <Textarea
                :id="field.name"
                rows="3"
                placeholder="このプロジェクトは何のためのものですか？"
                :model-value="field.state.value"
                @update:model-value="(v) => field.handleChange(String(v))"
              />
            </Field>
          </template>
        </form.Field>
      </section>

      <!-- Danger zone（設定モードのみ） -->
      <section
        v-if="isEdit && props.project"
        class="mb-4 rounded-[10px] border border-destructive/30 bg-destructive/5 p-6"
      >
        <h2 class="mb-4 text-base font-semibold text-destructive">Danger zone</h2>
        <div class="flex items-center justify-between gap-4">
          <div>
            <p class="text-sm font-medium">プロジェクトを削除</p>
            <p class="mt-0.5 text-xs text-muted-foreground">
              このプロジェクトとすべてのタスクを完全に削除します。
            </p>
          </div>
          <Button type="button" variant="destructive" @click="isDeleteOpen = true">削除</Button>
        </div>
      </section>

      <p v-if="submitError" role="alert" class="mb-4 text-sm text-destructive">
        {{ submitError }}
      </p>

      <!-- フッター -->
      <div class="flex items-center justify-end gap-2 pt-2">
        <Button type="button" variant="ghost" :disabled="isPending" @click="onCancel">
          キャンセル
        </Button>
        <form.Subscribe>
          <template #default="{ canSubmit, isSubmitting }">
            <Button type="submit" :disabled="!canSubmit || isSubmitting">
              {{
                isSubmitting
                  ? isEdit
                    ? '保存中…'
                    : '作成中…'
                  : isEdit
                    ? '変更を保存'
                    : 'プロジェクトを作成'
              }}
            </Button>
          </template>
        </form.Subscribe>
      </div>
    </form>

    <DeleteProjectDialog
      v-if="isEdit && props.project"
      :open="isDeleteOpen"
      :tenant-id="tenantId"
      :project="props.project"
      @update:open="isDeleteOpen = $event"
      @deleted="onDeleted"
    />
  </div>
</template>
