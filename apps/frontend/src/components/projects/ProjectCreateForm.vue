<script setup lang="ts">
import { useForm } from '@tanstack/vue-form';
import { type } from 'arktype';
import { useQueryClient } from '@tanstack/vue-query';
import { navigate } from 'vike/client/router';
import { computed, onMounted, ref } from 'vue';
import { Button, Field, Input, Textarea } from '@koyori-app/ui-vue';
import EmojiIconPicker from '@/components/projects/EmojiIconPicker.vue';
import {
  DEFAULT_STATUS_PREVIEW,
  PROJECT_KEY_PATTERN,
  suggestKey,
} from '@/components/projects/project-key';
import { apiClient } from '@/lib/api-vue-query';
import type { components } from '@/generated/api';

type ProjectResponse = components['schemas']['ProjectResponse'];

const LIST_PROJECTS_PATH = '/v1/tenants/{tenant_id}/projects' as const;

const props = defineProps<{
  tenantId: string;
  tenantSlug: string;
}>();

const queryClient = useQueryClient();
const submitError = ref<string | null>(null);
const keyEdited = ref(false);
const icon = ref<string | null>(null);
const nameInputRef = ref<InstanceType<typeof Input> | null>(null);

// koyori の Input は autofocus を root の span に落とすので、中の input に当てる
onMounted(() => {
  (nameInputRef.value?.$el as HTMLElement | undefined)?.querySelector('input')?.focus();
});

const nonBlankName = type('string').narrow((name) => name.trim().length >= 1);

const schema = type({
  name: nonBlankName,
  key: PROJECT_KEY_PATTERN,
  description: 'string',
});

const createMutation = apiClient.useMutation('post', LIST_PROJECTS_PATH);

const form = useForm({
  defaultValues: { name: '', key: '', description: '' },
  validators: { onSubmit: schema },
  onSubmit: async ({ value }) => {
    submitError.value = null;
    try {
      const saved: ProjectResponse = await createMutation.mutateAsync({
        params: { path: { tenant_id: props.tenantId } },
        body: {
          name: value.name.trim(),
          ...(value.key ? { key: value.key } : {}),
          ...(value.description.trim() ? { description: value.description.trim() } : {}),
          ...(icon.value ? { icon_emoji: icon.value } : {}),
        },
      });
      await queryClient.invalidateQueries({ queryKey: ['get', LIST_PROJECTS_PATH] });
      void navigate(`/${props.tenantSlug}/projects/${saved.key}/tasks`);
    } catch {
      submitError.value = 'プロジェクトを作成できませんでした';
    }
  },
});

const isPending = computed(() => createMutation.isPending.value);

function onCancel() {
  void navigate(`/${props.tenantSlug}/my-tasks`);
}
</script>

<template>
  <div class="mx-auto w-full max-w-[720px]">
    <div class="mb-6">
      <h1 class="mb-1 text-3xl font-bold tracking-tight">プロジェクトを作成</h1>
      <p class="text-sm text-muted-foreground">新しいタスクのためのワークスペースを用意します。</p>
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
            <EmojiIconPicker v-model="icon" />
          </div>

          <div class="min-w-0 flex-1">
            <form.Field name="name">
              <template #default="{ field }">
                <Field
                  :id="field.name"
                  label="名前"
                  description="サイドバーとパンくずに表示されます。"
                  :error="
                    field.state.meta.isTouched && field.state.meta.errors.length
                      ? '名前は必須です'
                      : ''
                  "
                >
                  <Input
                    ref="nameInputRef"
                    placeholder="プロジェクト名"
                    :value="field.state.value"
                    :on-blur="field.handleBlur"
                    :on-value-change="
                      (name: string) => {
                        field.handleChange(name);
                        if (!keyEdited) form.setFieldValue('key', suggestKey(name));
                      }
                    "
                  />
                </Field>
              </template>
            </form.Field>
          </div>
        </div>

        <div class="mb-5 max-w-[220px]">
          <form.Field name="key">
            <template #default="{ field }">
              <!-- 文字数は送信時に PROJECT_KEY_PATTERN で検証する -->
              <Field
                :id="field.name"
                label="キー"
                :description="`タスク番号の接頭辞（例: ${(field.state.value || 'ABC') + '-1'}）。空欄なら自動生成`"
                :error="
                  field.state.meta.isTouched && field.state.meta.errors.length
                    ? 'キーは 2〜10 文字で、先頭は大文字英字、残りは大文字英字または数字です'
                    : ''
                "
              >
                <Input
                  placeholder="ABC"
                  class="[&_input]:font-mono [&_input]:uppercase"
                  :value="field.state.value"
                  :on-blur="field.handleBlur"
                  :on-value-change="
                    (v: string) => {
                      keyEdited = true;
                      field.handleChange(v.toUpperCase());
                    }
                  "
                />
              </Field>
            </template>
          </form.Field>
        </div>

        <form.Field name="description">
          <template #default="{ field }">
            <Field :id="field.name" label="説明">
              <Textarea
                :rows="3"
                placeholder="このプロジェクトは何のためのものですか？"
                :value="field.state.value"
                :on-value-change="field.handleChange"
              />
            </Field>
          </template>
        </form.Field>
      </section>

      <!-- Workflow statuses プレビュー -->
      <section class="mb-4 rounded-[10px] border bg-card p-6">
        <h2 class="mb-1 text-base font-semibold">ワークフローステータス</h2>
        <p class="mb-4 text-sm text-muted-foreground">
          タスクが移動する列です。既定のセットで作成され、後から編集できます。
        </p>
        <ul class="overflow-hidden rounded-lg border">
          <li
            v-for="status in DEFAULT_STATUS_PREVIEW"
            :key="status.name"
            class="flex items-center gap-3 border-b px-3.5 py-2.5 last:border-b-0"
          >
            <span
              class="size-3.5 shrink-0 rounded"
              :style="{ backgroundColor: status.color }"
              aria-hidden="true"
            />
            <span class="flex-1 text-sm font-medium">{{ status.name }}</span>
            <span
              v-if="status.isDefault"
              class="rounded-full border px-2 py-px text-[11px] font-medium text-muted-foreground"
              >Default</span
            >
            <span
              v-if="status.isDone"
              class="rounded-full border px-2 py-px text-[11px] font-medium text-muted-foreground"
              >Done state</span
            >
          </li>
        </ul>
      </section>

      <p v-if="submitError" role="alert" class="mb-4 text-sm text-destructive">
        {{ submitError }}
      </p>

      <div class="flex items-center justify-end gap-2 pt-2">
        <Button
          type="button"
          variant="ghost"
          :disabled="isPending"
          label="キャンセル"
          @click="onCancel"
        />
        <form.Subscribe>
          <template #default="{ canSubmit, isSubmitting }">
            <Button
              type="submit"
              :disabled="!canSubmit || isSubmitting"
              :label="isSubmitting ? '作成中…' : 'プロジェクトを作成'"
            />
          </template>
        </form.Subscribe>
      </div>
    </form>
  </div>
</template>
