<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { Button, ConfirmDialog, Skeleton } from '@koyori-app/ui-vue';
import {
  queryErrorStatus,
  useAdminAuthMethodsQuery,
  useAdminDeletePasskeyMutation,
  useAdminDisconnectOAuthMutation,
  useAdminPasswordResetMutation,
  useAdminReset2faMutation,
  useAdminUpdateUserMutation,
  useAdminUsersQuery,
  useMeQuery,
} from '@/lib/api-vue-query';
import { adminErrorMessage, targetLabel, type AdminOperation } from '@/lib/admin-users';

const props = defineProps<{ userId: string }>();

const meQuery = useMeQuery();
const usersQuery = useAdminUsersQuery();
const authMethodsQuery = useAdminAuthMethodsQuery(() => props.userId);
const updateUser = useAdminUpdateUserMutation();
const passwordReset = useAdminPasswordResetMutation();
const reset2fa = useAdminReset2faMutation();
const deletePasskey = useAdminDeletePasskeyMutation();
const disconnectOAuth = useAdminDisconnectOAuthMutation();

const user = computed(() => usersQuery.data.value?.find((u) => u.id === props.userId));
const isSelf = computed(() => meQuery.data.value?.id === props.userId);
const target = computed(() => (user.value ? targetLabel(user.value) : ''));

const flash = ref('');
const error = ref('');

/** 確認待ちの操作。ダイアログは常に対象ユーザーと影響を本文に出す。 */
type Pending = {
  title: string;
  message: string;
  confirmLabel: string;
  destructive: boolean;
  operation: AdminOperation;
  success: string;
  run: () => Promise<unknown>;
};
const pending = ref<Pending | null>(null);

// 対象が変わったら、前の対象の確認待ち・通知・エラーを持ち越さない
watch(
  () => props.userId,
  () => {
    pending.value = null;
    flash.value = '';
    error.value = '';
  },
);

async function confirm() {
  const action = pending.value;
  pending.value = null;
  if (!action) return;
  flash.value = '';
  error.value = '';
  try {
    await action.run();
    flash.value = action.success;
    await Promise.all([usersQuery.refetch(), authMethodsQuery.refetch()]);
  } catch (e) {
    error.value = adminErrorMessage(action.operation, e);
  }
}

function askAdminChange(next: boolean) {
  const u = user.value!;
  pending.value = {
    title: next ? '管理者権限を付与しますか？' : '管理者権限を外しますか？',
    message: next
      ? `${target.value} を管理者にします。全ユーザーの閲覧・停止・認証復旧ができるようになります。`
      : `${target.value} から管理者権限を外します。管理画面を使えなくなります。`,
    confirmLabel: next ? '付与する' : '外す',
    destructive: true,
    operation: 'changeAdmin',
    success: next ? '管理者権限を付与しました。' : '管理者権限を外しました。',
    run: () => updateUser.mutateAsync({ params: { path: { id: u.id } }, body: { is_admin: next } }),
  };
}

function askSuspend(next: boolean) {
  const u = user.value!;
  pending.value = {
    title: next ? 'ユーザーを停止しますか？' : 'ユーザーの停止を解除しますか？',
    message: next
      ? `${target.value} を停止します。すべてのセッションとアクセストークンが失効し、サインインできなくなります。`
      : `${target.value} がふたたびサインインできるようになります。失効したセッションとトークンは戻りません。`,
    confirmLabel: next ? '停止する' : '解除する',
    destructive: next,
    operation: next ? 'suspend' : 'unsuspend',
    success: next ? 'ユーザーを停止しました。' : '停止を解除しました。',
    run: () =>
      updateUser.mutateAsync({ params: { path: { id: u.id } }, body: { is_suspended: next } }),
  };
}

function askPasswordReset() {
  const u = user.value!;
  pending.value = {
    title: 'パスワードリセットメールを送りますか？',
    message: `${target.value} 宛て（${u.email}）にパスワード再設定のリンクを送ります。現在のパスワードは、本人が再設定するまで有効なままです。`,
    confirmLabel: '送信する',
    destructive: false,
    operation: 'passwordReset',
    success: `${u.email} にリセットメールを送りました。`,
    run: () => passwordReset.mutateAsync({ params: { path: { id: u.id } }, body: {} }),
  };
}

function askReset2fa() {
  const u = user.value!;
  pending.value = {
    title: '2FA をリセットしますか？',
    message: `${target.value} の認証アプリとリカバリーコードを削除し、2FA を無効にします。すべてのセッションが失効し、再サインインが必要になります。`,
    confirmLabel: 'リセットする',
    destructive: true,
    operation: 'reset2fa',
    success: '2FA をリセットしました。',
    run: () => reset2fa.mutateAsync({ params: { path: { id: u.id } } }),
  };
}

function askDeletePasskey(passkey: { id: string; name: string }) {
  const u = user.value!;
  pending.value = {
    title: 'パスキーを削除しますか？',
    message: `${target.value} のパスキー「${passkey.name}」を削除します。このパスキーではサインインできなくなります。`,
    confirmLabel: '削除する',
    destructive: true,
    operation: 'deletePasskey',
    success: `パスキー「${passkey.name}」を削除しました。`,
    run: () =>
      deletePasskey.mutateAsync({ params: { path: { id: u.id, passkey_id: passkey.id } } }),
  };
}

function askDisconnectOAuth(provider: string) {
  const u = user.value!;
  const sameProvider = (authMethodsQuery.data.value?.oauth ?? []).filter(
    (o) => o.provider === provider,
  ).length;
  pending.value = {
    title: 'OAuth 連携を解除しますか？',
    message: `${target.value} の ${provider} の連携を解除します。この連携ではサインインできなくなります。${
      sameProvider > 1 ? `同じプロバイダーの連携 ${sameProvider} 件がすべて解除されます。` : ''
    }`,
    confirmLabel: '解除する',
    destructive: true,
    operation: 'disconnectOAuth',
    success: `${provider} の連携を解除しました。`,
    run: () => disconnectOAuth.mutateAsync({ params: { path: { id: u.id, provider } } }),
  };
}

const busy = computed(
  () =>
    updateUser.isPending.value ||
    passwordReset.isPending.value ||
    reset2fa.isPending.value ||
    deletePasskey.isPending.value ||
    disconnectOAuth.isPending.value,
);

function formatDate(value: string | null | undefined) {
  return value ? new Date(value).toLocaleString('ja-JP') : '未使用';
}
</script>

<template>
  <section class="flex flex-col gap-6">
    <Skeleton v-if="usersQuery.isLoading.value" height="8rem" />
    <p v-else-if="usersQuery.isError.value" role="alert" class="text-destructive text-sm">
      {{
        queryErrorStatus(usersQuery.error.value) === 403
          ? '管理者権限がないため、ユーザーを表示できません。'
          : 'ユーザーを取得できませんでした。'
      }}
    </p>
    <p v-else-if="!user" role="alert" class="text-sm">
      ユーザーが見つかりません。すでに削除された可能性があります。
      <a href="/admin/users" class="underline">一覧へ戻る</a>
    </p>
    <template v-else>
      <div class="flex flex-col gap-1">
        <h2 class="text-lg font-semibold">{{ user.username }}</h2>
        <p class="text-muted-foreground text-sm">{{ user.email }}</p>
        <p class="flex flex-wrap gap-2 text-xs">
          <span class="bg-secondary rounded px-2 py-0.5">{{
            user.is_admin ? '管理者' : '一般'
          }}</span>
          <span class="bg-secondary rounded px-2 py-0.5">{{
            user.is_suspended ? '停止中' : '有効'
          }}</span>
          <span class="bg-secondary rounded px-2 py-0.5">
            2FA {{ user.totp_enabled ? '有効' : '無効' }}
          </span>
          <span class="bg-secondary rounded px-2 py-0.5">
            パスワード {{ user.has_password ? 'あり' : 'なし' }}
          </span>
        </p>
      </div>

      <p v-if="flash" role="status" class="text-sm">{{ flash }}</p>
      <p v-if="error" role="alert" class="text-destructive text-sm">{{ error }}</p>

      <div class="flex flex-col gap-3 rounded-lg border p-4">
        <h3 class="text-base font-semibold">アカウント</h3>
        <div class="flex flex-wrap items-center gap-2">
          <Button
            type="button"
            variant="secondary"
            :label="user.is_admin ? '管理者権限を外す' : '管理者権限を付与'"
            :disabled="busy || (isSelf && user.is_admin)"
            @click="askAdminChange(!user.is_admin)"
          />
          <Button
            type="button"
            variant="secondary"
            :label="user.is_suspended ? '停止を解除' : '停止する'"
            :disabled="busy || (isSelf && !user.is_suspended)"
            @click="askSuspend(!user.is_suspended)"
          />
        </div>
        <p v-if="isSelf" class="text-muted-foreground text-xs">
          自分自身の管理者権限は外せず、停止もできません。最後の管理者の降格・停止も拒否されます。
        </p>
      </div>

      <div class="flex flex-col gap-3 rounded-lg border p-4">
        <h3 class="text-base font-semibold">認証の復旧</h3>
        <div class="flex flex-wrap items-center gap-2">
          <Button
            type="button"
            variant="secondary"
            label="パスワードリセットメールを送る"
            :disabled="busy"
            @click="askPasswordReset"
          />
          <Button
            type="button"
            variant="secondary"
            label="2FA をリセット"
            :disabled="busy || !user.totp_enabled"
            @click="askReset2fa"
          />
        </div>
        <p v-if="!user.totp_enabled" class="text-muted-foreground text-xs">
          2FA は有効になっていないため、リセットは不要です。
        </p>
      </div>

      <div class="flex flex-col gap-3 rounded-lg border p-4">
        <h3 class="text-base font-semibold">パスキー</h3>
        <Skeleton v-if="authMethodsQuery.isLoading.value" height="2.25rem" />
        <p v-else-if="authMethodsQuery.isError.value" role="alert" class="text-destructive text-sm">
          認証方法を取得できませんでした。
        </p>
        <p
          v-else-if="!authMethodsQuery.data.value?.passkeys.length"
          class="text-muted-foreground text-sm"
        >
          登録されたパスキーはありません。
        </p>
        <ul v-else class="divide-y">
          <li
            v-for="passkey in authMethodsQuery.data.value.passkeys"
            :key="passkey.id"
            class="flex flex-wrap items-center gap-2 py-2"
          >
            <span class="min-w-40 flex-1">
              <span class="block text-sm font-medium">{{ passkey.name }}</span>
              <span class="text-muted-foreground block text-xs">
                登録 {{ formatDate(passkey.created_at) }} / 最終使用
                {{ formatDate(passkey.last_used_at) }}
              </span>
            </span>
            <Button
              type="button"
              variant="danger"
              label="削除"
              :disabled="busy"
              @click="askDeletePasskey(passkey)"
            />
          </li>
        </ul>
      </div>

      <div class="flex flex-col gap-3 rounded-lg border p-4">
        <h3 class="text-base font-semibold">OAuth 連携</h3>
        <Skeleton v-if="authMethodsQuery.isLoading.value" height="2.25rem" />
        <p v-else-if="authMethodsQuery.isError.value" role="alert" class="text-destructive text-sm">
          認証方法を取得できませんでした。
        </p>
        <p
          v-else-if="!authMethodsQuery.data.value?.oauth.length"
          class="text-muted-foreground text-sm"
        >
          連携はありません。
        </p>
        <ul v-else class="divide-y">
          <li
            v-for="(connection, i) in authMethodsQuery.data.value.oauth"
            :key="i"
            class="flex flex-wrap items-center gap-2 py-2"
          >
            <span class="min-w-40 flex-1">
              <span class="block text-sm font-medium">{{ connection.provider }}</span>
              <span class="text-muted-foreground block text-xs">
                {{ connection.provider_login ?? connection.provider_email ?? '' }}
                <template v-if="connection.instance_url"> / {{ connection.instance_url }}</template>
                / 連携 {{ formatDate(connection.created_at) }}
              </span>
            </span>
            <Button
              type="button"
              variant="danger"
              label="解除"
              :disabled="busy"
              @click="askDisconnectOAuth(connection.provider)"
            />
          </li>
        </ul>
      </div>
    </template>

    <ConfirmDialog
      :open="pending !== null"
      :title="pending?.title ?? ''"
      :message="pending?.message ?? ''"
      :confirm-label="pending?.confirmLabel ?? ''"
      :destructive="pending?.destructive"
      :on-confirm="confirm"
      :on-cancel="() => (pending = null)"
    />
  </section>
</template>
