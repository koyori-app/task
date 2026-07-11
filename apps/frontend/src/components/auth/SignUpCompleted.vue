<script setup lang="ts">
import { Button } from '@/components/ui/button';

// #135: 登録 API は列挙対策のため新規/既存を区別せず 201 を返す。
// この成功表示も両者で完全に同一とし、既存アカウント利用者向けの導線
// （サインイン / パスワード再設定）を常に表示する。
defineProps<{ email: string }>();
</script>

<template>
  <div class="flex min-h-svh items-center justify-center">
    <div class="flex max-w-md flex-col items-center gap-6 px-4 text-center">
      <div class="flex flex-col gap-2">
        <h1 class="text-2xl font-bold">メールを送信しました</h1>
        <p class="text-muted-foreground text-sm">
          <span class="font-medium text-foreground">{{ email }}</span>
          にメールを送信しました。メール内の案内に沿って操作してください。
        </p>
      </div>
      <div class="flex flex-col items-center gap-3">
        <p class="text-muted-foreground text-sm">
          すでにアカウントをお持ちの場合は、サインインするか、パスワードを再設定できます。
        </p>
        <div class="flex items-center gap-3">
          <a href="/signin">
            <Button variant="outline">サインインへ</Button>
          </a>
          <a href="/auth/reset-password">
            <Button variant="ghost">パスワードを再設定する</Button>
          </a>
        </div>
      </div>
    </div>
  </div>
</template>
