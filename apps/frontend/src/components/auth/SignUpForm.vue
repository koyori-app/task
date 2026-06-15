<script setup lang="ts">
import { useForm } from '@tanstack/vue-form';
import { type } from 'arktype';
import { ref } from 'vue';
import PasswordInput from '@/components/auth/PasswordInput.vue';
import PasswordStrengthBar from '@/components/auth/PasswordStrengthBar.vue';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from '@/components/ui/field';
import { Input } from '@/components/ui/input';
import { usePasswordStrength } from '@/composables/usePasswordStrength';
import { arkMessage } from '@/lib/auth-validation';

const schema = type({
  username: 'string >= 3',
  email: 'string.email',
  password: 'string >= 8',
});

const hasSubmitted = ref(false); // TODO: use to display success message after registration
const passwordFocused = ref(false);
const passwordValue = ref('');
const { strength } = usePasswordStrength(passwordValue);

const form = useForm({
  defaultValues: { username: '', email: '', password: '' },
  validators: { onSubmit: schema },
  onSubmit: async ({ value }) => {
    hasSubmitted.value = true;
    // TODO: POST /v1/auth/register
    console.log('signup stub', value);
  },
});
</script>

<template>
  <div class="flex flex-col gap-6">
    <Card class="overflow-hidden p-0">
      <CardContent class="grid p-0 md:grid-cols-2">
        <form class="p-6 md:p-8" @submit.prevent="form.handleSubmit">
          <FieldGroup>
            <div class="flex flex-col items-center gap-2 text-center">
              <h1 class="text-2xl font-bold">アカウント作成</h1>
              <p class="text-muted-foreground text-sm text-balance">
                ユーザー名・メールアドレス・パスワードを入力してください
              </p>
            </div>
            <form.Field name="username" :validators="{ onBlur: type('string >= 3') }">
              <template #default="{ field }">
                <Field>
                  <FieldLabel :for="field.name">ユーザー名</FieldLabel>
                  <Input
                    :id="field.name"
                    :name="field.name"
                    type="text"
                    autocomplete="username"
                    :value="field.state.value"
                    @blur="field.handleBlur"
                    @input="(e: Event) => field.handleChange((e.target as HTMLInputElement).value)"
                  />
                  <div class="min-h-[1.25rem]">
                    <FieldError v-if="field.state.meta.errors.length > 0 && field.state.meta.isTouched">
                      {{ arkMessage(String(field.state.meta.errors[0])) }}
                    </FieldError>
                    <p v-else class="text-muted-foreground text-xs">
                      3文字以上で設定してください。
                    </p>
                  </div>
                </Field>
              </template>
            </form.Field>
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
                    <p v-else class="text-muted-foreground text-xs">
                      8文字以上で設定してください。
                    </p>
                  </div>
                </Field>
              </template>
            </form.Field>
            <form.Subscribe>
              <template #default="{ canSubmit, isSubmitting }">
                <Field>
                  <Button type="submit" class="w-full" :disabled="!canSubmit || isSubmitting">
                    {{ isSubmitting ? '登録中…' : 'アカウント作成' }}
                  </Button>
                </Field>
              </template>
            </form.Subscribe>
            <FieldDescription class="text-center">
              すでにアカウントをお持ちですか？
              <a href="/signin" class="underline underline-offset-4">サインイン</a>
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
