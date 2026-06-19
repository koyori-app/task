<script setup lang="ts">
import { useQueryClient } from '@tanstack/vue-query';
import { useForm } from '@tanstack/vue-form';
import { type } from 'arktype';
import { onMounted, ref } from 'vue';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from '@/components/ui/field';
import PasswordInput from '@/components/auth/PasswordInput.vue';
import { Input } from '@/components/ui/input';
import { meQueryOptions, useLoginMutation, useLogoutMutation } from '@/lib/api-vue-query';
import { arkMessage } from '@/lib/auth-validation';

const schema = type({
  email: 'string.email',
  password: 'string >= 8',
});

const queryClient = useQueryClient();
const loginMutation = useLoginMutation();
const logoutMutation = useLogoutMutation();
const submitError = ref<string | null>(null);
const isHydrated = ref(false);

onMounted(() => {
  isHydrated.value = true;
});

const form = useForm({
  defaultValues: { email: '', password: '' },
  validators: { onSubmit: schema },
  onSubmit: async ({ value }) => {
    submitError.value = null;
    try {
      const result = await loginMutation.mutateAsync({
        body: {
          email: value.email,
          password: value.password,
        },
      });

      if (result && typeof result === 'object' && 'requires_2fa' in result) {
        try {
          await logoutMutation.mutateAsync({} as never);
        } catch {
          // logout failure: still show the same unsupported-2FA message
        }
        submitError.value = '二要素認証は現在サポートされていません。';
        return;
      }

      await queryClient.invalidateQueries({ queryKey: meQueryOptions().queryKey });
      window.location.assign('/');
    } catch {
      submitError.value = 'メールアドレスまたはパスワードが正しくありません。';
    }
  },
});
</script>

<template>
  <div class="flex flex-col gap-6">
    <Card class="overflow-hidden p-0">
      <CardContent class="grid p-0 md:grid-cols-2">
        <form
          class="p-6 md:p-8"
          data-testid="signin-form"
          :data-hydrated="isHydrated ? 'true' : 'false'"
          @submit.prevent="form.handleSubmit"
        >
          <FieldGroup>
            <div class="flex flex-col items-center gap-2 text-center">
              <h1 class="text-2xl font-bold">おかえりなさい</h1>
              <p class="text-muted-foreground text-sm text-balance">
                メールアドレスを入力してサインインしてください
              </p>
            </div>
            <form.Field name="email" :validators="{ onBlur: type('string.email') }">
              <template #default="{ field }">
                <Field>
                  <FieldLabel :for="field.name">メールアドレス</FieldLabel>
                  <Input
                    :id="field.name"
                    :name="field.name"
                    type="email"
                    placeholder="m@example.com"
                    autocomplete="email"
                    :value="field.state.value"
                    @blur="field.handleBlur"
                    @input="(e: Event) => field.handleChange((e.target as HTMLInputElement).value)"
                  />
                  <FieldError class="min-h-[1.25rem]">
                    {{
                      field.state.meta.errors.length
                        ? arkMessage(String(field.state.meta.errors[0]))
                        : ''
                    }}
                  </FieldError>
                </Field>
              </template>
            </form.Field>
            <form.Field name="password" :validators="{ onBlur: type('string >= 8') }">
              <template #default="{ field }">
                <Field>
                  <FieldLabel :for="field.name">パスワード</FieldLabel>
                  <PasswordInput
                    :id="field.name"
                    :name="field.name"
                    autocomplete="current-password"
                    :model-value="field.state.value"
                    @update:model-value="field.handleChange"
                    @blur="field.handleBlur"
                  />
                  <FieldError class="min-h-[1.25rem]">
                    {{
                      field.state.meta.errors.length
                        ? arkMessage(String(field.state.meta.errors[0]))
                        : ''
                    }}
                  </FieldError>
                </Field>
              </template>
            </form.Field>
            <p v-if="submitError" class="text-destructive text-center text-sm">
              {{ submitError }}
            </p>
            <form.Subscribe>
              <template #default="{ isSubmitting }">
                <Field>
                  <Button type="submit" class="w-full" :disabled="isSubmitting">
                    {{ isSubmitting ? 'サインイン中…' : 'サインイン' }}
                  </Button>
                </Field>
              </template>
            </form.Subscribe>
            <FieldDescription class="text-center">
              アカウントをお持ちでない方は
              <a href="/signup" class="underline underline-offset-4">新規登録</a>
            </FieldDescription>
          </FieldGroup>
        </form>
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
