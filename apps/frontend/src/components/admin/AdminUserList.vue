<script setup lang="ts">
import { ref } from 'vue';
import { Button, Checkbox, Field, Input, Skeleton } from '@koyori-app/ui-vue';
import { useAdminCreateUserMutation, useAdminUsersQuery } from '@/lib/api-vue-query';
import { adminErrorMessage } from '@/lib/admin-users';

const usersQuery = useAdminUsersQuery();
const create = useAdminCreateUserMutation();

const creating = ref(false);
const form = ref({ username: '', email: '', password: '', is_admin: false });
const error = ref('');
const flash = ref('');

async function onCreate() {
  error.value = '';
  flash.value = '';
  try {
    const created = await create.mutateAsync({ body: { ...form.value, email_verified: true } });
    flash.value = `${created.username}（${created.email}）を作成しました。`;
    form.value = { username: '', email: '', password: '', is_admin: false };
    creating.value = false;
    await usersQuery.refetch();
  } catch (e) {
    error.value = adminErrorMessage('createUser', e);
  }
}
</script>

<template>
  <section class="flex flex-col gap-4">
    <div class="flex items-center justify-between gap-2">
      <h2 class="text-lg font-semibold">ユーザー一覧</h2>
      <Button
        v-if="!creating"
        type="button"
        variant="secondary"
        label="ユーザーを作成"
        @click="creating = true"
      />
    </div>

    <p v-if="flash" role="status" class="text-sm">{{ flash }}</p>

    <form
      v-if="creating"
      class="flex flex-col gap-3 rounded-lg border p-4"
      @submit.prevent="onCreate"
    >
      <Field id="admin-new-username" label="ユーザー名" description="3 文字以上" required>
        <Input :value="form.username" :on-value-change="(v: string) => (form.username = v)" />
      </Field>
      <Field id="admin-new-email" label="メールアドレス" required>
        <Input
          type="email"
          :value="form.email"
          :on-value-change="(v: string) => (form.email = v)"
        />
      </Field>
      <Field id="admin-new-password" label="初期パスワード" description="8 文字以上" required>
        <Input
          type="password"
          autocomplete="new-password"
          :value="form.password"
          :on-value-change="(v: string) => (form.password = v)"
        />
      </Field>
      <Checkbox
        label="管理者として作成する"
        :checked="form.is_admin"
        :on-checked-change="(v: boolean) => (form.is_admin = v)"
      />
      <p v-if="error" role="alert" class="text-destructive text-sm">{{ error }}</p>
      <div class="flex gap-2">
        <Button type="submit" label="作成する" :disabled="create.isPending.value" />
        <Button type="button" variant="tertiary" label="キャンセル" @click="creating = false" />
      </div>
    </form>

    <Skeleton v-if="usersQuery.isLoading.value" height="8rem" />
    <p v-else-if="usersQuery.isError.value" role="alert" class="text-destructive text-sm">
      ユーザー一覧を取得できませんでした。管理者権限がない可能性があります。
    </p>
    <ul v-else class="divide-y overflow-hidden rounded-lg border">
      <li v-for="user in usersQuery.data.value ?? []" :key="user.id">
        <a
          :href="`/admin/users/${user.id}`"
          class="flex flex-wrap items-center gap-2 p-4 hover:bg-accent/50"
        >
          <span class="min-w-40 flex-1">
            <span class="block text-sm font-medium">{{ user.username }}</span>
            <span class="text-muted-foreground block text-xs">{{ user.email }}</span>
          </span>
          <span v-if="user.is_admin" class="bg-secondary rounded px-2 py-0.5 text-xs">管理者</span>
          <span
            v-if="user.is_suspended"
            class="bg-secondary text-destructive rounded px-2 py-0.5 text-xs"
            >停止中</span
          >
          <span class="bg-secondary rounded px-2 py-0.5 text-xs">
            2FA {{ user.totp_enabled ? '有効' : '無効' }}
          </span>
        </a>
      </li>
    </ul>
  </section>
</template>
