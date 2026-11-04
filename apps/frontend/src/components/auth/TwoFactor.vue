<script setup lang="ts">
import { useQueryClient } from '@tanstack/vue-query';
import { onMounted, ref } from 'vue';
import { Button } from '@koyori-app/ui-vue';
import RecoveryCodesPanel from '@/components/auth/RecoveryCodesPanel.vue';
import TwoFactorSetupForm from '@/components/auth/TwoFactorSetupForm.vue';
import TwoFactorVerifyForm from '@/components/auth/TwoFactorVerifyForm.vue';
import {
  fetchClient,
  meQueryOptions,
  useTotpSetupMutation,
  useTotpVerifySetupMutation,
  useVerifyTwoFactorMutation,
} from '@/lib/api-vue-query';
import { safeRedirectAfter } from '@/lib/two-factor';

defineOptions({ name: 'TwoFactor' });

const props = defineProps<{ redirectAfter?: string | null }>();

type Phase = 'loading' | 'verify' | 'setup' | 'recovery' | 'expired' | 'error';

const queryClient = useQueryClient();
const setupMutation = useTotpSetupMutation();
const verifySetupMutation = useTotpVerifySetupMutation();
const verifyMutation = useVerifyTwoFactorMutation();

const phase = ref<Phase>('loading');
const setupData = ref<{ otpauth_uri: string; qr_code_png: string } | null>(null);
// リカバリーコードはこの ref にしか置かない（保存・URL・再取得はしない）。画面を離れれば消える
const recoveryCodes = ref<string[]>([]);
const submitError = ref<string | null>(null);
const locked = ref(false);

function statusOf(e: unknown): number | undefined {
  return (e as { response?: { status?: number } }).response?.status;
}

function messageOf(e: unknown): string | undefined {
  return (e as { error?: { message?: string } }).error?.message;
}

/** 検証済みの戻り先へ進む。/auth/me を取り直してから遷移する。 */
async function finish() {
  recoveryCodes.value = [];
  await queryClient.invalidateQueries({
    queryKey: meQueryOptions().queryKey,
    refetchType: 'all',
  });
  window.location.assign(safeRedirectAfter(props.redirectAfter));
}

/**
 * コード入力の失敗を画面に反映する。
 * 401 でも、コード違い（invalid-2fa-code）とセッション切れは別物として扱う。
 */
function handleCodeFailure(e: unknown): 'handled' | 'recheck' {
  const status = statusOf(e);
  if (status === 429) {
    locked.value = true;
    submitError.value = '試行回数の上限に達しました。しばらく待ってから、もう一度お試しください。';
    return 'handled';
  }
  if (status === 401 && messageOf(e) === 'invalid-2fa-code') {
    submitError.value = 'コードが正しくありません。';
    return 'handled';
  }
  if (status === 401) {
    phase.value = 'expired';
    return 'handled';
  }
  if (status === 403) return 'recheck';
  submitError.value = '確認に失敗しました。時間をおいて再度お試しください。';
  return 'handled';
}

/**
 * 今の画面を決める。/auth/me が 200 なら 2FA は済んでいる。403 なら半認証で、
 * セットアップを始めてみて 409（設定済み）なら検証、200 ならセットアップの画面にする。
 * OAuth から直接来た場合はログインの応答が無く、この判別しか手がかりが無い。
 */
async function init() {
  phase.value = 'loading';
  submitError.value = null;
  locked.value = false;
  try {
    const { response } = await fetchClient.GET('/v1/auth/me');
    if (response.ok) return await finish();
    if (response.status === 401) {
      phase.value = 'expired';
      return;
    }
  } catch {
    phase.value = 'error';
    return;
  }

  try {
    setupData.value = await setupMutation.mutateAsync({} as never);
    phase.value = 'setup';
  } catch (e) {
    const status = statusOf(e);
    if (status === 409) phase.value = 'verify';
    else if (status === 401) phase.value = 'expired';
    else phase.value = 'error';
  }
}

onMounted(init);

async function onVerify(input: { code: string } | { recovery_code: string }) {
  submitError.value = null;
  try {
    await verifyMutation.mutateAsync({ body: input });
  } catch (e) {
    if (handleCodeFailure(e) === 'recheck') await init();
    return;
  }
  await finish();
}

async function onSetupSubmit(code: string) {
  submitError.value = null;
  try {
    const result = await verifySetupMutation.mutateAsync({ body: { code } });
    // この時点でセッションは完全認証になる。コードを見せてから先へ進める
    recoveryCodes.value = result.recovery_codes;
    phase.value = 'recovery';
  } catch (e) {
    if (handleCodeFailure(e) === 'recheck') await init();
  }
}

/**
 * 半認証のセッションは /v1/auth/logout が 403 になる（AuthUser が半認証を拒むため）ので、
 * サーバー側では終了できない。サインイン画面へ戻り、入れ直しで置き換える。
 */
function cancel() {
  recoveryCodes.value = [];
  window.location.assign('/signin');
}
</script>

<template>
  <div class="bg-muted flex min-h-svh flex-col items-center justify-center p-6 md:p-10">
    <div class="w-full max-w-sm">
      <div class="koyori-card flex flex-col gap-6 p-6 md:p-8">
        <p
          v-if="phase === 'loading'"
          class="text-muted-foreground text-center text-sm"
          role="status"
        >
          読み込み中…
        </p>
        <TwoFactorVerifyForm
          v-else-if="phase === 'verify'"
          :pending="verifyMutation.isPending.value"
          :error="submitError"
          :locked="locked"
          @submit="onVerify"
        />
        <TwoFactorSetupForm
          v-else-if="phase === 'setup' && setupData"
          :otpauth-uri="setupData.otpauth_uri"
          :qr-code-png="setupData.qr_code_png"
          :pending="verifySetupMutation.isPending.value"
          :error="submitError"
          :locked="locked"
          @submit="onSetupSubmit"
        />
        <RecoveryCodesPanel
          v-else-if="phase === 'recovery'"
          :codes="recoveryCodes"
          @done="finish"
        />
        <div v-else-if="phase === 'expired'" class="flex flex-col gap-4 text-center">
          <h1 class="text-xl font-bold">セッションの有効期限が切れました</h1>
          <p class="text-muted-foreground text-sm">もう一度サインインしてください。</p>
          <Button class="w-full" label="サインインへ" @click="cancel" />
        </div>
        <div v-else class="flex flex-col gap-4 text-center">
          <h1 class="text-xl font-bold">2 段階認証を開始できません</h1>
          <p class="text-muted-foreground text-sm">
            時間をおいて再度お試しいただくか、サインインし直してください。
          </p>
          <Button class="w-full" label="再読み込み" @click="init" />
        </div>
        <button
          v-if="phase === 'verify' || phase === 'setup' || phase === 'error'"
          type="button"
          class="text-muted-foreground text-center text-sm underline underline-offset-4"
          @click="cancel"
        >
          キャンセルしてサインインへ戻る
        </button>
      </div>
    </div>
  </div>
</template>
