<script setup lang="ts">
import { useForm } from '@tanstack/vue-form';
import { type } from 'arktype';
import { ref, watch } from 'vue';
import { Button, Dialog, Field, Input, Textarea } from '@koyori-app/ui-vue';
import { useTenantStore } from '@/stores/tenant';

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ 'update:open': [open: boolean] }>();
const store = useTenantStore();
const submitError = ref<string | null>(null);
const displayIdEdited = ref(false);

const slugify = (value: string) =>
  value
    .normalize('NFKD')
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 50);

const nonBlankName = type('string').narrow((name) => name.trim().length >= 1);

const schema = type({
  name: nonBlankName,
  display_id: /^[a-z0-9]+(?:-[a-z0-9]+)*$/,
  description: 'string',
  icon_url: 'string',
});

const form = useForm({
  defaultValues: { name: '', display_id: '', description: '', icon_url: '' },
  validators: { onSubmit: schema },
  onSubmit: async ({ value }) => {
    submitError.value = null;
    try {
      const tenant = await store.createTenant({
        name: value.name.trim(),
        display_id: value.display_id,
        ...(value.description.trim() ? { description: value.description.trim() } : {}),
        ...(value.icon_url.trim() ? { icon_url: value.icon_url.trim() } : {}),
      });
      emit('update:open', false);
      form.reset();
      displayIdEdited.value = false;
      window.location.assign(`/${tenant.display_id}`);
    } catch (error) {
      submitError.value = error instanceof Error ? error.message : 'テナントを作成できませんでした';
    }
  },
});

watch(
  () => props.open,
  (open) => {
    if (!open) {
      submitError.value = null;
      form.reset();
      displayIdEdited.value = false;
    }
  },
);
</script>

<template>
  <div class="contents [--koyori-dialog-width:32rem]">
    <Dialog
      :open="open"
      title="テナントを作成"
      description="新しいワークスペースの名前と表示IDを入力してください。"
      :on-close="() => emit('update:open', false)"
    >
      <form id="create-tenant-form" class="space-y-4" @submit.prevent="form.handleSubmit">
        <div class="flex flex-col gap-6">
          <form.Field name="name">
            <template #default="{ field }">
              <Field
                :id="field.name"
                label="名前"
                :error="
                  field.state.meta.isTouched && field.state.meta.errors.length
                    ? '名前は必須です'
                    : ''
                "
              >
                <Input
                  :value="field.state.value"
                  :on-blur="field.handleBlur"
                  :on-value-change="
                    (name: string) => {
                      field.handleChange(name);
                      if (!displayIdEdited) form.setFieldValue('display_id', slugify(name));
                    }
                  "
                />
              </Field>
            </template>
          </form.Field>
          <form.Field name="display_id">
            <template #default="{ field }">
              <Field
                :id="field.name"
                label="表示ID"
                description="英小文字・数字・ハイフンのみ"
                :error="
                  field.state.meta.isTouched && field.state.meta.errors.length
                    ? '有効な表示IDを入力してください'
                    : ''
                "
              >
                <Input
                  :value="field.state.value"
                  :on-blur="field.handleBlur"
                  :on-value-change="
                    (v: string) => {
                      displayIdEdited = true;
                      field.handleChange(v.toLowerCase());
                    }
                  "
                />
              </Field>
            </template>
          </form.Field>
          <form.Field name="description">
            <template #default="{ field }">
              <Field :id="field.name" label="説明（任意）">
                <Textarea :value="field.state.value" :on-value-change="field.handleChange" />
              </Field>
            </template>
          </form.Field>
          <form.Field name="icon_url">
            <template #default="{ field }">
              <Field :id="field.name" label="アイコンURL（任意）">
                <Input
                  type="url"
                  :value="field.state.value"
                  :on-value-change="field.handleChange"
                />
              </Field>
            </template>
          </form.Field>
        </div>
        <p v-if="submitError" role="alert" class="text-sm text-destructive">{{ submitError }}</p>
      </form>
      <template #actions>
        <Button
          type="button"
          variant="tertiary"
          label="キャンセル"
          @click="emit('update:open', false)"
        />
        <form.Subscribe>
          <template #default="{ canSubmit, isSubmitting }">
            <Button
              type="submit"
              form="create-tenant-form"
              :disabled="!canSubmit || isSubmitting"
              :label="isSubmitting ? '作成中…' : '作成'"
            />
          </template>
        </form.Subscribe>
      </template>
    </Dialog>
  </div>
</template>
