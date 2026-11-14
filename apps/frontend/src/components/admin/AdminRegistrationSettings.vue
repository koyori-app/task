<script setup lang="ts">
import { ref } from 'vue';
import { Skeleton, Switch } from '@koyori-app/ui-vue';
import {
  useAdminSystemSettingsQuery,
  useAdminUpdateSystemSettingsMutation,
} from '@/lib/api-vue-query';
import { adminErrorMessage } from '@/lib/admin-users';

const settingsQuery = useAdminSystemSettingsQuery();
const update = useAdminUpdateSystemSettingsMutation();
const flash = ref('');
const error = ref('');

async function onChange(enabled: boolean) {
  flash.value = '';
  error.value = '';
  try {
    const next = await update.mutateAsync({ body: { user_registration_enabled: enabled } });
    flash.value = next.user_registration_enabled
      ? '一般ユーザー登録を有効にしました。'
      : '一般ユーザー登録を無効にしました。';
    await settingsQuery.refetch();
  } catch (e) {
    error.value = adminErrorMessage('updateSettings', e);
  }
}
</script>

<template>
  <section class="flex flex-col gap-3">
    <h2 class="text-lg font-semibold">一般ユーザー登録</h2>
    <p class="text-muted-foreground text-sm">
      無効にすると、メールアドレスと外部プロバイダーのどちらからの新規登録も受け付けません。既存ユーザーのサインインと、管理者によるユーザー作成は影響を受けません。
    </p>
    <Skeleton v-if="settingsQuery.isLoading.value" height="2rem" />
    <p v-else-if="settingsQuery.isError.value" role="alert" class="text-destructive text-sm">
      システム設定を取得できませんでした。管理者権限がない可能性があります。
    </p>
    <Switch
      v-else
      label="一般ユーザー登録を受け付ける"
      :checked="settingsQuery.data.value?.user_registration_enabled ?? false"
      :disabled="update.isPending.value"
      :on-checked-change="onChange"
    />
    <p v-if="flash" role="status" class="text-sm">{{ flash }}</p>
    <p v-if="error" role="alert" class="text-destructive text-sm">{{ error }}</p>
  </section>
</template>
