<script setup lang="ts">
import { useForm } from '@tanstack/vue-form';
import { type } from 'arktype';
import { onMounted, ref } from 'vue';
import { Button, Field, PasswordInput } from '@koyori-app/ui-vue';
import { Card, CardContent } from '@/components/ui/card';
import HydrationSafeForm from '@/components/HydrationSafeForm.vue';
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
      <Button label="サインインページへ" />
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
      <Button variant="tertiary" label="再設定リンクを再取得する" />
    </a>
  </div>
  <Card v-else class="overflow-hidden p-0">
    <CardContent class="p-0">
      <HydrationSafeForm v-slot="{ isHydrated }" class="p-6 md:p-8" @submit="form.handleSubmit">
        <div class="flex flex-col gap-6">
          <div class="flex flex-col items-center gap-2 text-center">
            <h1 class="text-2xl font-bold">新しいパスワードを設定</h1>
            <p class="text-muted-foreground text-sm text-balance">
              新しいパスワードを入力してください
            </p>
          </div>
          <form.Field name="newPassword" :validators="{ onBlur: type('string >= 8') }">
            <template #default="{ field }">
              <!-- 強度バーを出している間（入力中）はエラーを出さない -->
              <Field
                :id="field.name"
                label="新しいパスワード"
                description="8文字以上で設定してください。"
                :error="
                  !(passwordValue.length > 0 && passwordFocused) &&
                  field.state.meta.errors.length > 0 &&
                  field.state.meta.isTouched
                    ? arkMessage(String(field.state.meta.errors[0]))
                    : ''
                "
              >
                <!-- koyori の PasswordInput は onFocus / onBlur を持たず root が span なので、root の捕捉フェーズで focus / blur を拾う -->
                <PasswordInput
                  :name="field.name"
                  autocomplete="new-password"
                  :value="field.state.value"
                  :on-value-change="
                    (v: string) => {
                      field.handleChange(v);
                      passwordValue = v;
                    }
                  "
                  @focus.capture="passwordFocused = true"
                  @blur.capture="
                    () => {
                      field.handleBlur();
                      passwordFocused = false;
                    }
                  "
                />
                <PasswordStrengthBar
                  v-if="
                    passwordValue.length > 0 && (passwordFocused || !field.state.meta.errors.length)
                  "
                  :strength="strength"
                />
              </Field>
            </template>
          </form.Field>
          <p v-if="submitError" class="text-destructive text-center text-sm">
            {{ submitError }}
          </p>
          <form.Subscribe>
            <template #default="{ canSubmit, isSubmitting }">
              <Button
                type="submit"
                class="w-full"
                :disabled="!canSubmit || isSubmitting || !isHydrated"
                :label="isSubmitting ? '設定中…' : 'パスワードを再設定する'"
              />
            </template>
          </form.Subscribe>
        </div>
      </HydrationSafeForm>
    </CardContent>
  </Card>
</template>
