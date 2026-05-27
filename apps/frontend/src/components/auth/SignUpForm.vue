<script setup lang="ts">
import { ref } from "vue"
import { navigate } from "vike/client/router"
import { createApi } from "@/lib/api"
import { ResponseError } from "@/generated/api/runtime"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"

const username = ref("")
const email = ref("")
const password = ref("")
const errorMessage = ref("")
const isSubmitting = ref(false)

function mapRegisterError(err: unknown): string {
  if (err instanceof ResponseError) {
    const status = err.response.status
    if (status === 422 || status === 400) {
      return "入力内容に誤りがあります"
    }
    if (status === 409) {
      return "このメールアドレスは既に使用されています"
    }
  }
  return "エラーが発生しました。しばらくしてからお試しください"
}

async function onSubmit(event: SubmitEvent) {
  event.preventDefault()
  errorMessage.value = ""
  isSubmitting.value = true
  try {
    await createApi().register({
      registerRequest: {
        username: username.value,
        email: email.value,
        password: password.value,
      },
    })
    await navigate("/signin")
  } catch (err) {
    errorMessage.value = mapRegisterError(err)
  } finally {
    isSubmitting.value = false
  }
}
</script>

<template>
  <div class="flex flex-col gap-6">
    <Card class="overflow-hidden p-0">
      <CardContent class="grid p-0 md:grid-cols-2">
        <form class="p-6 md:p-8" @submit="onSubmit">
          <FieldGroup>
            <div class="flex flex-col items-center gap-2 text-center">
              <h1 class="text-2xl font-bold">
                アカウント作成
              </h1>
              <p class="text-muted-foreground text-sm text-balance">
                ユーザー名・メールアドレス・パスワードを入力してください
              </p>
            </div>
            <div
              v-if="errorMessage"
              class="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive"
              role="alert"
            >
              {{ errorMessage }}
            </div>
            <Field>
              <FieldLabel for="username">
                ユーザー名
              </FieldLabel>
              <Input
                id="username"
                v-model="username"
                type="text"
                autocomplete="username"
                required
              />
            </Field>
            <Field>
              <FieldLabel for="email">
                メールアドレス
              </FieldLabel>
              <Input
                id="email"
                v-model="email"
                type="email"
                placeholder="m@example.com"
                autocomplete="email"
                required
              />
            </Field>
            <Field>
              <FieldLabel for="password">
                パスワード
              </FieldLabel>
              <Input
                id="password"
                v-model="password"
                type="password"
                autocomplete="new-password"
                required
              />
              <FieldDescription>
                8文字以上で設定してください。
              </FieldDescription>
            </Field>
            <Field>
              <Button type="submit" class="w-full" :disabled="isSubmitting">
                {{ isSubmitting ? "登録中…" : "アカウント作成" }}
              </Button>
            </Field>
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
          >
        </div>
      </CardContent>
    </Card>
  </div>
</template>
