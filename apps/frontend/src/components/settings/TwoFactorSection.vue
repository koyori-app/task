<script setup lang="ts">
import { useQueryClient } from '@tanstack/vue-query';
import { onUnmounted, ref } from 'vue';
import { Button } from '@koyori-app/ui-vue';
import RecoveryCodesPanel from '@/components/auth/RecoveryCodesPanel.vue';
import TwoFactorSetupForm from '@/components/auth/TwoFactorSetupForm.vue';
import TwoFactorVerifyForm from '@/components/auth/TwoFactorVerifyForm.vue';
import {
  meQueryOptions,
  useDeleteTotpMutation,
  useRegenerateRecoveryCodesMutation,
  useTotpSetupMutation,
  useTotpVerifySetupMutation,
} from '@/lib/api-vue-query';
import type { components } from '@/generated/api';

defineProps<{ user: components['schemas']['UserResponse'] }>();

type Mode = 'idle' | 'setup' | 'disable' | 'regenerate' | 'recovery';

const queryClient = useQueryClient();
const setupMutation = useTotpSetupMutation();
const verifySetupMutation = useTotpVerifySetupMutation();
const deleteMutation = useDeleteTotpMutation();
const regenerateMutation = useRegenerateRecoveryCodesMutation();

const mode = ref<Mode>('idle');
const setupData = ref<{ otpauth_uri: string; qr_code_png: string } | null>(null);
// リカバリーコードはこの ref にしか置かない（保存・URL・再取得はしない）。閉じれば消える
const recoveryCodes = ref<string[]>([]);
const error = ref<string | null>(null);
const locked = ref(false);
// 429 後に送信を止める時間。制限そのものは backend が守るので、画面は連打を防ぐだけ
const LOCK_MS = 30_000;
let lockTimer: ReturnType<typeof setTimeout> | undefined;
onUnmounted(() => clearTimeout(lockTimer));

function statusOf(e: unknown): number | undefined {
  return (e as { response?: { status?: number } }).response?.status;
}
function messageOf(e: unknown): string | undefined {
  return (e as { error?: { message?: string } }).error?.message;
}

function refetchMe() {
  return queryClient.invalidateQueries({ queryKey: meQueryOptions().queryKey, refetchType: 'all' });
}

function open(next: Mode) {
  error.value = null;
  mode.value = next;
}

function close() {
  recoveryCodes.value = [];
  setupData.value = null;
  open('idle');
}

/** 失敗の表示だけを決める。状態（mode）は変えない。 */
function fail(e: unknown, enforcedReason = false) {
  const status = statusOf(e);
  if (status === 429) {
    clearTimeout(lockTimer);
    locked.value = true;
    lockTimer = setTimeout(() => (locked.value = false), LOCK_MS);
    error.value = '試行回数の上限に達しました。しばらく待ってから、もう一度お試しください。';
  } else if (status === 401 && messageOf(e) === 'invalid-2fa-code') {
    error.value = 'コードが正しくありません。';
  } else if (status === 403 && enforcedReason) {
    error.value =
      '所属するテナントが 2 段階認証を必須にしているため、無効化できません。管理者に相談してください。';
  } else if (status === 401) {
    error.value = 'セッションの有効期限が切れました。サインインし直してください。';
  } else {
    error.value = '操作に失敗しました。時間をおいて再度お試しください。';
  }
}

async function startSetup() {
  // 初回コードの検証中に新しいセットアップを始めると、backend が秘密を入れ替え、
  // 古い秘密で通った検証が新しい秘密を有効にしてしまう
  if (verifySetupMutation.isPending.value) return;
  error.value = null;
  try {
    setupData.value = await setupMutation.mutateAsync({} as never);
    mode.value = 'setup';
  } catch (e) {
    // 409 は他の画面で設定済み。状態を取り直す
    if (statusOf(e) === 409) await refetchMe();
    else fail(e);
  }
}

async function onSetupSubmit(code: string) {
  error.value = null;
  try {
    const result = await verifySetupMutation.mutateAsync({ body: { code } });
    recoveryCodes.value = result.recovery_codes;
    verifySetupMutation.reset();
    setupData.value = null;
    mode.value = 'recovery';
  } catch (e) {
    fail(e);
    return;
  }
  await refetchMe();
}

async function onDisable(input: { code: string } | { recovery_code: string }) {
  error.value = null;
  try {
    await deleteMutation.mutateAsync({ body: input });
  } catch (e) {
    fail(e, true);
    return;
  }
  close();
  await refetchMe();
}

async function onRegenerate(input: { code: string } | { recovery_code: string }) {
  error.value = null;
  if (!('code' in input)) return;
  try {
    const result = await regenerateMutation.mutateAsync({ body: { code: input.code } });
    recoveryCodes.value = result.recovery_codes;
    regenerateMutation.reset();
    mode.value = 'recovery';
  } catch (e) {
    fail(e);
    return;
  }
  await refetchMe();
}
</script>

<template>
  <section class="flex flex-col gap-4" data-testid="two-factor-section">
    <div class="flex flex-col gap-1">
      <h3 class="font-semibold">2 段階認証（認証アプリ）</h3>
      <p class="text-muted-foreground text-sm">
        {{
          user.totp_enabled
            ? '有効です。'
            : '無効です。有効にすると、サインイン時に認証アプリのコードも求めます。'
        }}
      </p>
    </div>

    <RecoveryCodesPanel v-if="mode === 'recovery'" :codes="recoveryCodes" @done="close" />
    <TwoFactorSetupForm
      v-else-if="mode === 'setup' && setupData"
      embedded
      :otpauth-uri="setupData.otpauth_uri"
      :qr-code-png="setupData.qr_code_png"
      :pending="verifySetupMutation.isPending.value"
      :error="error"
      :locked="locked"
      @submit="onSetupSubmit"
    />
    <template v-else-if="mode === 'disable' || mode === 'regenerate'">
      <TwoFactorVerifyForm
        embedded
        :allow-recovery="mode === 'disable'"
        :submit-label="mode === 'disable' ? '2 段階認証を無効にする' : '再生成する'"
        :pending="deleteMutation.isPending.value || regenerateMutation.isPending.value"
        :error="error"
        :locked="locked"
        @submit="mode === 'disable' ? onDisable($event) : onRegenerate($event)"
      />
      <Button
        variant="secondary"
        label="キャンセル"
        :disabled="deleteMutation.isPending.value || regenerateMutation.isPending.value"
        @click="close"
      />
    </template>
    <div v-else class="flex flex-wrap gap-2">
      <template v-if="user.totp_enabled">
        <Button variant="secondary" label="リカバリーコードを再生成" @click="open('regenerate')" />
        <Button variant="secondary" label="無効にする" @click="open('disable')" />
      </template>
      <Button
        v-else
        label="有効にする"
        :disabled="setupMutation.isPending.value || verifySetupMutation.isPending.value"
        @click="startSetup"
      />
    </div>
    <p v-if="error && mode === 'idle'" class="text-destructive text-sm" role="alert">{{ error }}</p>

    <Button
      v-if="mode === 'setup'"
      variant="secondary"
      label="キャンセル"
      :disabled="verifySetupMutation.isPending.value"
      @click="close"
    />
  </section>
</template>
