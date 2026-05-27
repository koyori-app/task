<script setup lang="ts">
import type { HTMLAttributes } from "vue"
import { ref } from "vue"
import { navigate } from "vike/client/router"
import { cn } from "@/lib/utils"
import { createApi } from "@/lib/api"
import { ResponseError } from "@/generated/api/runtime"
import { Button } from "@/components/ui/button"
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"

const props = defineProps<{
  class?: HTMLAttributes["class"]
}>()

const email = ref("")
const password = ref("")
const errorMessage = ref("")
const isSubmitting = ref(false)

function loginErrorMessage(error: unknown): string {
  if (error instanceof ResponseError) {
    const status = error.response.status
    if (status === 401) {
      return "メールアドレスまたはパスワードが正しくありません"
    }
    if (status === 403) {
      return "メールアドレスの確認が完了していません"
    }
  }
  return "エラーが発生しました。しばらくしてからお試しください"
}

async function onSubmit(event: Event) {
  event.preventDefault()
  errorMessage.value = ""
  isSubmitting.value = true

  try {
    await createApi().login({
      loginRequest: {
        email: email.value,
        password: password.value,
      },
    })
    await navigate("/")
  } catch (error) {
    errorMessage.value = loginErrorMessage(error)
  } finally {
    isSubmitting.value = false
  }
}
</script>

<template>
  <form :class="cn('flex flex-col gap-6', props.class)" @submit="onSubmit">
    <FieldGroup>
      <div class="flex flex-col items-center gap-1 text-center">
        <h1 class="text-2xl font-bold">
          Login to your account
        </h1>
        <p class="text-muted-foreground text-sm text-balance">
          Enter your email below to login to your account
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
          Email
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
          Password
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
        <Button type="submit" :disabled="isSubmitting">
          {{ isSubmitting ? "Logging in..." : "Login" }}
        </Button>
      </Field>
      <Field>
        <FieldDescription class="text-center">
          Don't have an account?
          <a href="/signup" class="underline underline-offset-4">Sign up</a>
        </FieldDescription>
      </Field>
    </FieldGroup>
  </form>
</template>
