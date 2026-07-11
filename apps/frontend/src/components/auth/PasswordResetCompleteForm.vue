<script setup lang="ts">
import { useForm } from '@tanstack/vue-form';
import { type } from 'arktype';
import { onMounted, ref } from 'vue';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { Field, FieldError, FieldGroup, FieldLabel } from '@/components/ui/field';
import HydrationSafeForm from '@/components/HydrationSafeForm.vue';
import PasswordInput from '@/components/auth/PasswordInput.vue';
import PasswordStrengthBar from '@/components/auth/PasswordStrengthBar.vue';
import { usePasswordStrength } from '@/composables/usePasswordStrength';
import {
  usePasswordResetCompleteMutation,
  usePasswordResetVerifyMutation,
} from '@/lib/api-vue-query';
import { arkMessage } from '@/lib/auth-validation';

const props = defineProps<{ token: string }>();

const schema = type({
  newPassword: 'string >= 8',
});

const verifyMutation = usePasswordResetVerifyMutation();
const completeMutation = usePasswordResetCompleteMutation();

const tokenState = ref<'verifying' | 'valid' | 'invalid'>('verifying');
const submitError = ref<string | null>(null);
const completed = ref(false);
const passwordFocused = ref(false);
const passwordValue = ref('');
const { strength } = usePasswordStrength(passwordValue);

onMounted(async () => {
  try {
    await verifyMutation.mutateAsync({ body: { token: props.token }, parseAs: 'text' });
    tokenState.value = 'valid';
  } catch {
    tokenState.value = 'invalid';
  }
});

const form = useForm({
  defaultValues: { newPassword: '' },
  validators: { onSubmit: schema },
  onSubmit: async ({ value }) => {
    submitError.value = null;
    try {
      await completeMutation.mutateAsync({
        body: { token: props.token, new_password: value.newPassword },
      });
      completed.value = true;
    } catch (e) {
      const status = (e as { response?: { status?: number } }).response?.status;
      if (status === 400 || status === 404) {
        tokenState.value = 'invalid';
      } else {
        submitError.value = '設定に失敗しました。時間をおいて再度お試しください。';
      }
    }
  },
});
</script>

<template>
  <div v-if="completed" class="flex flex-col items-center gap-4 p-6 text-center md:p-8">
    <h1 class="text-2xl font-bold">パスワードを再設定しました</h1>
    <p class="text-muted-foreground text-sm">新しいパスワードでサインインしてください。</p>
    <a href="/signin">
      <Button>サインインページへ</Button>
    </a>
  </div>
  <div
    v-else-if="tokenState === 'verifying'"
    class="text-muted-foreground p-6 text-center text-sm md:p-8"
  >
    リンクを確認しています…
  </div>
  <div
    v-else-if="tokenState === 'invalid'"
    class="flex flex-col items-center gap-4 p-6 text-center md:p-8"
  >
    <h1 class="text-2xl font-bold">リンクが無効です</h1>
    <p class="text-muted-foreground text-sm">
      パスワード再設定用のリンクが無効か、有効期限が切れています。お手数ですが再度リクエストしてください。
    </p>
    <a href="/auth/reset-password">
      <Button variant="outline">再設定リンクを再取得する</Button>
    </a>
  </div>
  <Card v-else class="overflow-hidden p-0">
    <CardContent class="p-0">
      <HydrationSafeForm v-slot="{ isHydrated }" class="p-6 md:p-8" @submit="form.handleSubmit">
        <FieldGroup>
          <div class="flex flex-col items-center gap-2 text-center">
            <h1 class="text-2xl font-bold">新しいパスワードを設定</h1>
            <p class="text-muted-foreground text-sm text-balance">
              新しいパスワードを入力してください
            </p>
          </div>
          <form.Field name="newPassword" :validators="{ onBlur: type('string >= 8') }">
            <template #default="{ field }">
              <Field>
                <FieldLabel :for="field.name">新しいパスワード</FieldLabel>
                <PasswordInput
                  :id="field.name"
                  :name="field.name"
                  autocomplete="new-password"
                  :model-value="field.state.value"
                  @update:model-value="
                    (v: string) => {
                      field.handleChange(v);
                      passwordValue = v;
                    }
                  "
                  @focus="passwordFocused = true"
                  @blur="
                    () => {
                      field.handleBlur();
                      passwordFocused = false;
                    }
                  "
                />
                <div class="min-h-[1.5rem]">
                  <PasswordStrengthBar
                    v-if="
                      passwordValue.length > 0 &&
                      (passwordFocused || !field.state.meta.errors.length)
                    "
                    :strength="strength"
                  />
                  <FieldError
                    v-else-if="field.state.meta.errors.length > 0 && field.state.meta.isTouched"
                  >
                    {{ arkMessage(String(field.state.meta.errors[0])) }}
                  </FieldError>
                  <p v-else class="text-muted-foreground text-xs">8文字以上で設定してください。</p>
                </div>
              </Field>
            </template>
          </form.Field>
          <p v-if="submitError" class="text-destructive text-center text-sm">
            {{ submitError }}
          </p>
          <form.Subscribe>
            <template #default="{ canSubmit, isSubmitting }">
              <Field>
                <Button
                  type="submit"
                  class="w-full"
                  :disabled="!canSubmit || isSubmitting || !isHydrated"
                >
                  {{ isSubmitting ? '設定中…' : 'パスワードを再設定する' }}
                </Button>
              </Field>
            </template>
          </form.Subscribe>
        </FieldGroup>
      </HydrationSafeForm>
    </CardContent>
  </Card>
</template>
