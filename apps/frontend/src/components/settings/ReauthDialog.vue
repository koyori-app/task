<script setup lang="ts">
import { PhCaretRight, PhFingerprint, PhLink, PhLockKey } from '@phosphor-icons/vue';
import { computed, ref, watch, type Component } from 'vue';
import { Alert, Button, Dialog, Field, PasswordInput, Spinner } from '@koyori-app/ui-vue';
import { useReauthGate } from '@/composables/useReauthGate';
import { fetchClient } from '@/lib/api-vue-query';
import { markNotice, REAUTH_RESUME_NOTICE } from '@/lib/one-time-notice';
import { providerLabel, startOAuth } from '@/lib/oauth-providers';
import {
  reauthOAuthLabel,
  reauthOperationLabel,
  reauthWindowLabel,
  type ReauthOAuthMethod,
} from '@/lib/reauth';
import {
  getPasskeyAssertion,
  isWebAuthnAvailable,
  type RequestChallengeJSON,
} from '@/lib/webauthn';

/**
 * 本人確認（再認証）の dialog。使える手段を同じ重さで並べ、選んだ手段だけをその場で開く。
 * 開け閉めは useReauthGate の requireRecentAuth が持つ。
 */
const props = defineProps<{
  /** 連携で確かめた後に戻す画面（フロントの相対パス）。 */
  returnPath: string;
}>();

const { state, finish } = useReauthGate();

type Method =
  | { key: 'password'; label: string; hint: string; icon: Component }
  | { key: 'passkey'; label: string; hint: string; icon: Component }
  | { key: string; label: string; hint: string; icon: Component; oauth: ReauthOAuthMethod };

const methods = computed<Method[]>(() => {
  const status = state.status;
  if (!status) return [];
  const list: Method[] = [];
  if (status.methods.password) {
    list.push({
      key: 'password',
      label: 'パスワード',
      hint: 'この画面で入力します',
      icon: PhLockKey,
    });
  }
  // パスキーを持っていても、このブラウザで使えなければ出さない
  if (status.methods.passkey && isWebAuthnAvailable()) {
    list.push({
      key: 'passkey',
      label: 'パスキー',
      hint: 'ブラウザの画面で、この端末や鍵を使います',
      icon: PhFingerprint,
    });
  }
  for (const oauth of status.methods.oauth) {
    const label = reauthOAuthLabel(oauth);
    list.push({
      key: `oauth:${oauth.connection_provider}:${oauth.instance_url ?? ''}`,
      label,
      // 名は instance で見分けるが、補足はプロバイダーの名だけにして短くする
      hint: `${providerLabel(oauth.provider)} の画面へ移り、戻ってから続けます`,
      icon: PhLink,
      oauth,
    });
  }
  return list;
});

const selected = ref<string | null>(null);
const password = ref('');
const passwordError = ref<string | null>(null);
const error = ref<string | null>(null);
const busy = ref(false);

// 問いごとに中身を戻す。開いたままの dialog に次の問いが来た時も、前の知らせや選びを持ち越さない
watch(
  () => [state.open, state.seq] as const,
  ([open]) => {
    if (!open) return;
    selected.value = null;
    password.value = '';
    passwordError.value = null;
    error.value = state.notice;
    busy.value = false;
  },
  { immediate: true },
);

const operation = computed(() => (state.resume ? reauthOperationLabel(state.resume) : ''));
const windowLabel = computed(() => reauthWindowLabel(state.status?.window_secs ?? 300));
const hasMethods = computed(() => methods.value.length > 0);
const title = computed(() => (hasMethods.value ? '本人確認' : '本人確認ができません'));
const description = computed(() =>
  hasMethods.value
    ? `${operation.value}前に、本人であることを確かめます。確かめた後 ${windowLabel.value}は、もう一度求めません。`
    : `${operation.value}には本人確認が要りますが、使える方法がありません。サインインし直すと、その後 ${windowLabel.value}はこの操作ができます。`,
);
const selectedOAuth = computed(() => {
  const method = methods.value.find((m) => m.key === selected.value);
  return method && 'oauth' in method ? method : null;
});

function messageOf(e: unknown): string | undefined {
  return (e as { message?: string } | undefined)?.message;
}

function choose(method: Method) {
  if (busy.value) return;
  error.value = null;
  passwordError.value = null;
  selected.value = method.key;
  if (method.key === 'passkey') void byPasskey();
}

async function byPassword() {
  if (!password.value) {
    passwordError.value = 'パスワードを入力してください。';
    return;
  }
  busy.value = true;
  passwordError.value = null;
  const { error: e } = await fetchClient.POST('/v1/auth/reauth/password', {
    body: { password: password.value },
    parseAs: 'text',
  });
  busy.value = false;
  if (!e) {
    finish(true);
    return;
  }
  passwordError.value =
    messageOf(e) === 'invalid-current-password'
      ? 'パスワードが違います。もう一度入力してください。'
      : '確かめられませんでした。時間をおいて再度お試しください。';
}

async function byPasskey() {
  busy.value = true;
  try {
    const { data, error: e } = await fetchClient.POST('/v1/auth/reauth/passkey/start');
    if (e || !data) throw new Error('start');
    const credential = await getPasskeyAssertion(data as unknown as RequestChallengeJSON);
    const { error: finishError } = await fetchClient.POST('/v1/auth/reauth/passkey/finish', {
      // openapi は credential の中身の形を決めない（Object）。中身は backend が検める
      body: { credential: credential as unknown as Record<string, never> },
      parseAs: 'text',
    });
    if (finishError) throw new Error('finish');
    finish(true);
  } catch {
    // 取り消し・時間切れ・別の鍵を区別せずに知らせ、一覧から選び直させる
    error.value = 'パスキーで確かめられませんでした。もう一度試すか、ほかの方法を選んでください。';
    selected.value = null;
  } finally {
    busy.value = false;
  }
}

function byOAuth(oauth: ReauthOAuthMethod) {
  if (!state.resume) return;
  // 戻った画面で続ける操作と、確かめに使った連携（失敗の知らせに使う）を、このタブに置く
  markNotice(REAUTH_RESUME_NOTICE, JSON.stringify({ ...state.resume, via: oauth.provider }));
  busy.value = true;
  startOAuth(oauth.provider, {
    redirectAfter: props.returnPath,
    errorRedirectAfter: props.returnPath,
    instanceUrl: oauth.instance_url ?? undefined,
    reauth: true,
  });
}

async function signInAgain() {
  busy.value = true;
  // サインインで本人確認の印が立つ。サインアウトに失敗しても、サインイン画面へは移す
  await fetchClient.POST('/v1/auth/logout').catch(() => undefined);
  window.location.assign('/signin');
}

function cancel() {
  if (busy.value && selected.value === 'passkey') return;
  finish(false);
}
</script>

<template>
  <Dialog :open="state.open" :title="title" :description="description" :on-close="cancel">
    <div class="flex flex-col gap-3" data-testid="reauth-dialog">
      <Alert v-if="error" :message="error" />
      <Alert
        v-if="!hasMethods"
        variant="info"
        message="パスワード・パスキー・サインインに使える連携のどれも無いため、この画面では確かめられません。"
      />
      <template v-for="method in methods" :key="method.key">
        <button
          type="button"
          class="flex w-full items-center gap-3 rounded-lg border p-3 text-left disabled:opacity-60"
          :class="
            selected === method.key
              ? 'border-(--koyori-color-selection-indicator) bg-secondary'
              : ''
          "
          :aria-pressed="selected === method.key"
          :disabled="busy"
          :data-testid="`reauth-method-${method.key.split(':')[0]}`"
          @click="choose(method)"
        >
          <span class="bg-secondary flex size-8 shrink-0 items-center justify-center rounded-lg">
            <component :is="method.icon" class="size-4" />
          </span>
          <span class="min-w-0 flex-1">
            <span class="block text-sm font-medium">{{ method.label }}</span>
            <span class="text-muted-foreground block text-xs">{{ method.hint }}</span>
          </span>
          <PhCaretRight v-if="selected !== method.key" class="text-muted-foreground size-3.5" />
        </button>
        <form
          v-if="method.key === 'password' && selected === 'password'"
          class="flex flex-col gap-3"
          @submit.prevent="byPassword"
        >
          <Field id="reauth-password" label="パスワード" :error="passwordError ?? ''">
            <PasswordInput
              autocomplete="current-password"
              :value="password"
              :on-value-change="(v: string) => (password = v)"
            />
          </Field>
        </form>
        <div
          v-if="method.key === 'passkey' && selected === 'passkey' && busy"
          class="flex flex-col gap-1"
        >
          <Spinner label="ブラウザの画面で確かめています" />
          <p class="text-muted-foreground text-xs">
            ブラウザが出す画面で、パスキーを選んでください。終わると、そのまま{{
              operation
            }}操作へ進みます。
          </p>
        </div>
      </template>
      <p v-if="selectedOAuth" class="text-muted-foreground text-xs" data-testid="reauth-oauth-note">
        {{ selectedOAuth.label }} の画面へ移ります。確かめた後この画面へ戻り、{{
          operation
        }}操作を続けます。
      </p>
    </div>

    <template #actions>
      <Button
        type="button"
        variant="tertiary"
        label="キャンセル"
        :disabled="busy"
        @click="cancel"
      />
      <Button
        v-if="selected === 'password'"
        type="button"
        :disabled="busy"
        :label="busy ? '確かめています…' : '確かめる'"
        data-testid="reauth-password-submit"
        @click="byPassword"
      />
      <Button
        v-else-if="selectedOAuth"
        type="button"
        :disabled="busy"
        :label="`${selectedOAuth.label} へ進む`"
        data-testid="reauth-oauth-submit"
        @click="byOAuth(selectedOAuth.oauth)"
      />
      <Button
        v-else-if="!hasMethods"
        type="button"
        :disabled="busy"
        label="サインインし直す"
        data-testid="reauth-sign-in-again"
        @click="signInAgain"
      />
    </template>
  </Dialog>
</template>
