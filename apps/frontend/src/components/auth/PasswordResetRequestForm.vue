<script setup lang="ts">
import { useForm } from '@tanstack/vue-form';
import { type } from 'arktype';
import { ref } from 'vue';
import { Button, Field, Input } from '@koyori-app/ui-vue';
import { Card, CardContent } from '@/components/ui/card';
import HydrationSafeForm from '@/components/HydrationSafeForm.vue';
import { usePasswordResetRequestMutation } from '@/lib/api-vue-query';
import { arkMessage } from '@/lib/auth-validation';

const schema = type({
  email: 'string.email',
});

const requestMutation = usePasswordResetRequestMutation();
const submitError = ref<string | null>(null);
const requestedEmail = ref<string | null>(null);

const form = useForm({
  defaultValues: { email: '' },
  validators: { onSubmit: schema },
  onSubmit: async ({ value }) => {
    submitError.value = null;
    try {
      await requestMutation.mutateAsync({ body: { email: value.email } });
      requestedEmail.value = value.email;
    } catch (e) {
      if ((e as { response?: { status?: number } }).response?.status === 429) {
        submitError.value = 'リクエストが連続しています。時間をおいて再度お試しください。';
      } else {
        submitError.value = '送信に失敗しました。時間をおいて再度お試しください。';
      }
    }
  },
});
</script>

<template>
  <div v-if="requestedEmail" class="flex flex-col items-center gap-4 p-6 text-center md:p-8">
    <h1 class="text-2xl font-bold">メールを送信しました</h1>
    <p class="text-muted-foreground text-sm">
      <span class="font-medium text-foreground">{{ requestedEmail }}</span>
      が登録済みの場合、パスワード再設定用のリンクを送信しました。メールをご確認ください。
    </p>
    <a href="/signin">
      <Button variant="ghost" label="サインインページへ戻る" />
    </a>
  </div>
  <Card v-else class="overflow-hidden p-0">
    <CardContent class="p-0">
      <HydrationSafeForm v-slot="{ isHydrated }" class="p-6 md:p-8" @submit="form.handleSubmit">
        <div class="flex flex-col gap-6">
          <div class="flex flex-col items-center gap-2 text-center">
            <h1 class="text-2xl font-bold">パスワード再設定</h1>
            <p class="text-muted-foreground text-sm text-balance">
              登録済みのメールアドレスを入力してください。再設定用のリンクを送信します
            </p>
          </div>
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
          <p v-if="submitError" class="text-destructive text-center text-sm">
            {{ submitError }}
          </p>
          <form.Subscribe>
            <template #default="{ canSubmit, isSubmitting }">
              <Button
                type="submit"
                class="w-full"
                :disabled="!canSubmit || isSubmitting || !isHydrated"
                :label="isSubmitting ? '送信中…' : '再設定リンクを送信'"
              />
            </template>
          </form.Subscribe>
          <p class="text-muted-foreground text-center text-sm">
            <a href="/signin" class="underline underline-offset-4">サインインページへ戻る</a>
          </p>
        </div>
      </HydrationSafeForm>
    </CardContent>
  </Card>
</template>
