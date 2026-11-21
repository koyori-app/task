<script setup lang="ts">
import { usePageContext } from 'vike-vue/usePageContext';
import { computed, onMounted, ref, watch } from 'vue';
import OAuthConsentForm from '@/components/settings/OAuthConsentForm.vue';
import {
  isSessionRejected,
  useApproveAuthorizationMutation,
  useDenyAuthorizationMutation,
  useMeQuery,
  usePreviewAuthorizationMutation,
  useTenantsQuery,
} from '@/lib/api-vue-query';
import { signInUrl } from '@/composables/useRedirectAfter';
import {
  RESUME_PARAM,
  authorizeErrorMessage,
  parseAuthorizeQuery,
  readAuthorizeQuery,
  removeAuthorizeQuery,
  resumeUrl,
  stashAuthorizeQuery,
  type AuthorizationParams,
  type AuthorizationPreview,
} from '@/lib/oauth-authorize';
import type { TokenScope } from '@/lib/personal-tokens';

// MCP の OAuth の同意画面（認可エンドポイント）。MCP クライアントが、認可の要求を query に付けて
// 利用者のブラウザーをここへ送る。認証ガードの外に置き（+Layout の isAuthPage）、未ログインなら
// サインインへ送って戻ってくる。
//
// 要求の URL は `redirect_uri=https://…` や `scope=read:task` を含み、サインインの戻り先の検査
// （`:` を含む値を捨てる）を通らない。そこで、開いた時点で query を sessionStorage に置き、URL を
// `/oauth/authorize?resume={キー}` に替える。以後、要求の中身は常に sessionStorage から読む。
const pageContext = usePageContext() as {
  urlParsed?: { searchOriginal?: string | null; search?: Record<string, string> };
};

/** このページを開いた URL の query。 */
function openedSearch(): URLSearchParams {
  const parsed = pageContext.urlParsed;
  if (parsed?.searchOriginal) return new URLSearchParams(parsed.searchOriginal);
  if (parsed?.search) return new URLSearchParams(parsed.search);
  return new URLSearchParams(window.location.search);
}

/** ほかのページの枠の中で開かれているか。確かめられないときは、枠の中として扱う。 */
function isFramed(): boolean {
  try {
    return window.top !== window.self;
  } catch {
    return true;
  }
}

// sessionStorage と window を読むので、mount のあとに決める（SSR の出力と食い違わせない）
const ready = ref(false);
const framed = ref(false);
const resumeKey = ref<string | null>(null);
const request = ref<AuthorizationParams | null>(null);

onMounted(() => {
  framed.value = isFramed();
  const search = openedSearch();
  let key = search.get(RESUME_PARAM);
  if (key === null && search.size > 0) {
    key = stashAuthorizeQuery(search.toString());
    window.history.replaceState(window.history.state, '', resumeUrl(key));
  }
  // 別のタブで開いた、承認のあとで戻るボタンを押した等で中身が無ければ、やり直しを案内する
  const stored = key === null ? null : readAuthorizeQuery(key);
  if (key !== null && stored !== null) {
    resumeKey.value = key;
    request.value = parseAuthorizeQuery(new URLSearchParams(stored));
  }
  ready.value = true;
});

const meQuery = useMeQuery();
const signedIn = computed(() => meQuery.data.value !== undefined);
// `/me` 以外の API が 401 を返すと、共通の処理が戻り先なしのサインインへ送ってしまう。
// ほかの API は、`/me` が成功してから呼ぶ
const tenantsQuery = useTenantsQuery({ enabled: signedIn });
const previewAuthorization = usePreviewAuthorizationMutation();
const approveAuthorization = useApproveAuthorizationMutation();
const denyAuthorization = useDenyAuthorizationMutation();

const preview = ref<AuthorizationPreview | null>(null);
const previewError = ref<string | null>(null);
const submitError = ref<string | null>(null);
const done = ref(false);

const meLoadFailed = computed(
  () => meQuery.isError.value && !meQuery.data.value && !isSessionRejected(meQuery.error.value),
);

watch(
  () =>
    ready.value &&
    !framed.value &&
    resumeKey.value !== null &&
    request.value !== null &&
    isSessionRejected(meQuery.error.value) &&
    !meQuery.isFetching.value,
  (signedOut) => {
    if (!signedOut || resumeKey.value === null) return;
    window.location.assign(signInUrl(resumeUrl(resumeKey.value)));
  },
  { immediate: true },
);

type ApiError = { response?: { status?: number }; error?: { message?: unknown } };

let previewRequested = false;
watch(
  () => ready.value && !framed.value && signedIn.value && request.value !== null,
  async (canPreview) => {
    if (!canPreview || previewRequested || request.value === null) return;
    previewRequested = true;
    try {
      preview.value = await previewAuthorization.mutateAsync({ body: request.value });
    } catch (e) {
      const err = e as ApiError;
      previewError.value =
        err.response?.status === 400
          ? authorizeErrorMessage(err.error?.message)
          : '接続の要求を確かめられませんでした。時間をおいて、クライアントからやり直してください。';
    }
  },
  { immediate: true },
);

/** 承認できるのは、自分が所属するテナント（PAT の発行と同じ条件）。 */
const tenants = computed(() =>
  (tenantsQuery.data.value ?? [])
    .filter((tenant) => tenant.membership !== 'Guest')
    .map((tenant) => ({ id: tenant.id, name: tenant.name })),
);

const pending = computed(
  () => approveAuthorization.isPending.value || denyAuthorization.isPending.value,
);

/** クライアントへ戻る。保存した要求は、遷移の直前にだけ消す（失敗したときは残す）。 */
function leave(redirectTo: string) {
  if (resumeKey.value !== null) removeAuthorizeQuery(resumeKey.value);
  done.value = true;
  window.location.assign(redirectTo);
}

function submitErrorMessage(e: unknown, fallback: string): string {
  const err = e as ApiError;
  const status = err.response?.status;
  if (status === 403 && err.error?.message === '2fa-setup-required') {
    return '選択したテナントのどれかが 2 段階認証を必須にしています。2 段階認証を設定してから、もう一度お試しください。';
  }
  // 理由を名指しする 403 だけ原因を伝え、ほかの 403（Origin の検査など）は汎用の文にする
  // （PAT の発行の画面と同じ分け方）
  if (status === 403 && err.error?.message === 'tenant-membership-missing') {
    return '選択したテナントに所属していないため、接続できません。';
  }
  if (status === 400) return authorizeErrorMessage(err.error?.message);
  return fallback;
}

async function approve(choice: { tenantIds: string[]; scopes: TokenScope[] }) {
  if (request.value === null) return;
  submitError.value = null;
  try {
    const { redirect_to } = await approveAuthorization.mutateAsync({
      body: { ...request.value, tenant_ids: choice.tenantIds, scopes: choice.scopes },
    });
    leave(redirect_to);
  } catch (e) {
    submitError.value = submitErrorMessage(
      e,
      '許可できませんでした。時間をおいて、もう一度お試しください。',
    );
  }
}

async function deny() {
  if (request.value === null) return;
  submitError.value = null;
  try {
    const { redirect_to } = await denyAuthorization.mutateAsync({ body: request.value });
    leave(redirect_to);
  } catch (e) {
    submitError.value = submitErrorMessage(
      e,
      '拒否を伝えられませんでした。このタブを閉じれば、接続は行われません。',
    );
  }
}
</script>

<template>
  <div class="bg-muted flex min-h-svh flex-col items-center justify-center p-6 md:p-10">
    <div
      class="koyori-card flex w-full max-w-md flex-col gap-4 text-center [--koyori-card-padding:1.5rem]"
    >
      <p v-if="!ready" class="text-muted-foreground text-sm">読み込み中…</p>
      <!-- 枠の中では承認させない（応答のヘッダーと二重に守る） -->
      <template v-else-if="framed">
        <h1 class="text-xl font-bold">このページは埋め込みでは開けません</h1>
        <p class="text-muted-foreground text-sm">
          接続を許可するには、クライアントからこのページを開き直してください。
        </p>
      </template>
      <template v-else-if="!request">
        <h1 class="text-xl font-bold">リンクが正しくありません</h1>
        <p class="text-muted-foreground text-sm">クライアントからやり直してください。</p>
      </template>
      <p v-else-if="meLoadFailed" class="text-sm" role="alert">
        サーバーに接続できませんでした。時間をおいてやり直してください。
      </p>
      <p v-else-if="!meQuery.data.value" class="text-muted-foreground text-sm">読み込み中…</p>
      <p v-else-if="done" class="text-muted-foreground text-sm">
        クライアントに戻っています。このタブは閉じてかまいません。
      </p>
      <template v-else-if="previewError">
        <h1 class="text-xl font-bold">接続できません</h1>
        <p class="text-sm" role="alert">{{ previewError }}</p>
        <p class="text-muted-foreground text-xs break-all">
          クライアント: <code data-testid="rejected-client-id">{{ request.client_id }}</code>
        </p>
      </template>
      <p v-else-if="tenantsQuery.isError.value" class="text-sm" role="alert">
        テナントの一覧を取得できませんでした。再読み込みしてください。
      </p>
      <p v-else-if="!preview || !tenantsQuery.data.value" class="text-muted-foreground text-sm">
        読み込み中…
      </p>
      <OAuthConsentForm
        v-else
        :preview="preview"
        :tenants="tenants"
        :username="meQuery.data.value.username"
        :pending="pending"
        :error="submitError"
        @approve="approve"
        @deny="deny"
      />
    </div>
  </div>
</template>
