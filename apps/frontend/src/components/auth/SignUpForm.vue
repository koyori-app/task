<script setup lang="ts">
import { useForm } from '@tanstack/vue-form';
import { type } from 'arktype';
import { ref } from 'vue';
import EmailNotVerified from '@/components/auth/EmailNotVerified.vue';
import OAuthButtons from '@/components/auth/OAuthButtons.vue';
import PasswordStrengthBar from '@/components/auth/PasswordStrengthBar.vue';
import HydrationSafeForm from '@/components/HydrationSafeForm.vue';
import { Button, Field, Input, PasswordInput } from '@koyori-app/ui-vue';
import { Card, CardContent } from '@/components/ui/card';
import { usePasswordStrength } from '@/composables/usePasswordStrength';
import { useRegisterMutation } from '@/lib/api-vue-query';
import { arkMessage } from '@/lib/auth-validation';

const schema = type({
  username: 'string >= 3',
  email: 'string.email',
  password: 'string >= 8',
});

const registerMutation = useRegisterMutation();
const submitError = ref<string | null>(null);
const registeredEmail = ref<string | null>(null);
const passwordFocused = ref(false);
const passwordValue = ref('');
const { strength } = usePasswordStrength(passwordValue);

const form = useForm({
  defaultValues: { username: '', email: '', password: '' },
  validators: { onSubmit: schema },
  onSubmit: async ({ value }) => {
    submitError.value = null;
    try {
      await registerMutation.mutateAsync({
        body: {
          username: value.username,
          email: value.email,
          password: value.password,
        },
        parseAs: 'text',
      });
      registeredEmail.value = value.email;
    } catch {
      submitError.value = '登録に失敗しました。時間をおいて再度お試しください。';
    }
  },
});
</script>

<template>
  <EmailNotVerified
    v-if="registeredEmail"
    :email="registeredEmail"
    back-href="/signin"
    reset-href="/auth/reset-password"
  />
  <div v-else class="flex flex-col gap-6">
    <Card class="overflow-hidden p-0">
      <CardContent class="grid p-0 md:grid-cols-2">
        <HydrationSafeForm
          v-slot="{ isHydrated }"
          class="p-6 md:p-8"
          @submit="() => form.handleSubmit()"
        >
          <div class="flex flex-col gap-6">
            <div class="flex flex-col items-center gap-2 text-center">
              <h1 class="text-2xl font-bold">アカウント作成</h1>
              <p class="text-muted-foreground text-sm text-balance">
                ユーザー名・メールアドレス・パスワードを入力してください
              </p>
            </div>
            <form.Field name="username" :validators="{ onBlur: type('string >= 3') }">
              <template #default="{ field }">
                <Field
                  :id="field.name"
                  label="ユーザー名"
                  description="3文字以上で設定してください。"
                  :error="
                    field.state.meta.errors.length > 0 && field.state.meta.isTouched
                      ? arkMessage(String(field.state.meta.errors[0]))
                      : ''
                  "
                >
                  <Input
                    :name="field.name"
                    type="text"
                    autocomplete="username"
                    :value="field.state.value"
                    :on-value-change="field.handleChange"
                    :on-blur="field.handleBlur"
                  />
                </Field>
              </template>
            </form.Field>
            <form.Field name="email" :validators="{ onBlur: type('string.email') }">
              <template #default="{ field }">
                <Field
                  :id="field.name"
                  label="メールアドレス"
                  :error="
                    field.state.meta.errors.length
                      ? arkMessage(String(field.state.meta.errors[0]))
                      : ''
                  "
                >
                  <Input
                    :name="field.name"
                    type="email"
                    placeholder="m@example.com"
                    autocomplete="email"
                    :value="field.state.value"
                    :on-value-change="field.handleChange"
                    :on-blur="field.handleBlur"
                  />
                </Field>
              </template>
            </form.Field>
            <form.Field name="password" :validators="{ onBlur: type('string >= 8') }">
              <template #default="{ field }">
                <!-- 強度バーを出している間（入力中）はエラーを出さない -->
                <Field
                  :id="field.name"
                  label="パスワード"
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
                      passwordValue.length > 0 &&
                      (passwordFocused || !field.state.meta.errors.length)
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
                  :label="isSubmitting ? '登録中…' : 'アカウント作成'"
                />
              </template>
            </form.Subscribe>
            <OAuthButtons redirect-after="/" error-redirect-after="/signup" />
            <p class="text-muted-foreground text-center text-sm">
              すでにアカウントをお持ちですか？
              <a href="/signin" class="underline underline-offset-4">サインイン</a>
            </p>
          </div>
        </HydrationSafeForm>
        <div class="bg-muted relative hidden md:block">
          <img
            src="/placeholder.svg"
            alt=""
            class="absolute inset-0 h-full w-full object-cover dark:brightness-[0.2] dark:grayscale"
          />
        </div>
      </CardContent>
    </Card>
  </div>
</template>
