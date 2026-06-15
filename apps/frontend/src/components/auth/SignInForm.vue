<script setup lang="ts">
import { useForm } from '@tanstack/vue-form';
import { type } from 'arktype';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from '@/components/ui/field';
import PasswordInput from '@/components/auth/PasswordInput.vue';
import { Input } from '@/components/ui/input';

const schema = type({
  email: 'string.email',
  password: 'string >= 8',
});

function arkMessage(msg: string): string {
  if (msg.includes('email address')) return 'メールアドレスの形式が正しくありません';
  if (msg.includes('at least length 8')) return '8文字以上で入力してください';
  return msg;
}

const form = useForm({
  defaultValues: { email: '', password: '' },
  validators: { onSubmit: schema },
  onSubmit: async ({ value }) => {
    // TODO: POST /v1/auth/login
    console.log('signin stub', value);
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
            <form.Subscribe>
              <template #default="{ canSubmit, isSubmitting }">
                <Field>
                  <Button type="submit" class="w-full" :disabled="!canSubmit || isSubmitting">
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
