<script setup lang="ts">
import { ref } from "vue"
import { navigate } from "vike/client/router"
import { apiClient } from "@/lib/api"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"

const email = ref("")
const password = ref("")
const errorMessage = ref("")
const isSubmitting = ref(false)

function loginErrorMessage(response: Response): string {
  const status = response.status
  if (status === 401) {
    return "メールアドレスまたはパスワードが正しくありません"
  }
  if (status === 403) {
    return "メールアドレスの確認が完了していません"
  }
  return "エラーが発生しました。しばらくしてからお試しください"
}

async function onSubmit(event: Event) {
  event.preventDefault()
  errorMessage.value = ""
  isSubmitting.value = true

  try {
    const { error, response } = await apiClient.POST("/v1/auth/login", {
      body: {
        email: email.value,
        password: password.value,
      },
    })
    if (error) {
      errorMessage.value = loginErrorMessage(response)
      return
    }
    await navigate("/")
  } catch {
    errorMessage.value = "エラーが発生しました。しばらくしてからお試しください"
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
                おかえりなさい
              </h1>
              <p class="text-muted-foreground text-sm text-balance">
                メールアドレスを入力してサインインしてください
              </p>
            </div>
            <div
              v-if="errorMessage"
              role="alert"
              class="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive"
            >
              {{ errorMessage }}
            </div>
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
                autocomplete="current-password"
                required
              />
            </Field>
            <Field>
              <Button type="submit" class="w-full" :disabled="isSubmitting">
                {{ isSubmitting ? "サインイン中…" : "サインイン" }}
              </Button>
            </Field>
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
          >
        </div>
      </CardContent>
    </Card>
  </div>
</template>
