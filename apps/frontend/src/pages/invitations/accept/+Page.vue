<script setup lang="ts">
import { usePageContext } from 'vike-vue/usePageContext';
import { computed, ref } from 'vue';
import { Button } from '@koyori-app/ui-vue';
import { apiClient, useMeQuery } from '@/lib/api-vue-query';
import { INVITATION_ACCEPT_RETURN } from '@/lib/invitation-accept';
import { markNotice } from '@/lib/one-time-notice';

// 認証ガードの外に置く（+Layout の isAuthPage）。未登録の受け手にも招待の中身を見せ、
// 未ログインならこの URL を覚えてサインインへ送る（戻ってくると承諾できる）
const pageContext = usePageContext();
const token = (
  (pageContext as { urlParsed?: { search?: Record<string, string> } }).urlParsed?.search?.token ??
  ''
).trim();

const previewQuery = apiClient.useQuery(
  'post',
  '/v1/invitations/preview',
  { body: { token } },
  { enabled: token !== '', retry: false, staleTime: Infinity },
);
const preview = computed(() => previewQuery.data.value);
const previewStatus = computed(
  () => (previewQuery.error.value as { response?: { status?: number } } | null)?.response?.status,
);

const meQuery = useMeQuery();
const signedOut = computed(() => meQuery.isError.value && !meQuery.isFetching.value);
// API の照合と同じく大文字小文字は区別しない（`normalize_email`）
const addressMatches = computed(
  () =>
    !!preview.value &&
    meQuery.data.value?.email.trim().toLowerCase() === preview.value.email.toLowerCase(),
);

const acceptMutation = apiClient.useMutation('post', '/v1/invitations/accept');
const acceptError = ref<string | null>(null);

function signIn() {
  markNotice(INVITATION_ACCEPT_RETURN, `${window.location.pathname}${window.location.search}`);
  window.location.assign('/signin');
}

async function accept() {
  if (!preview.value) return;
  acceptError.value = null;
  try {
    await acceptMutation.mutateAsync({ body: { token } });
    // テナント一覧・ストアを取り直すため、画面ごと読み込み直す
    window.location.assign(`/${preview.value.tenant_display_id}`);
  } catch (e) {
    switch ((e as { response?: { status?: number } }).response?.status) {
      case 409:
        acceptError.value = '既にこのテナントのメンバーです。';
        break;
      case 410:
        acceptError.value = '招待の期限が切れています。招待した人に送り直してもらってください。';
        break;
      case 404:
        acceptError.value = 'この招待は使えなくなっています。';
        break;
      default:
        acceptError.value = '参加できませんでした。時間をおいてやり直してください。';
    }
  }
}
</script>

<template>
  <div class="bg-muted flex min-h-svh flex-col items-center justify-center p-6 md:p-10">
    <div
      class="koyori-card flex w-full max-w-sm flex-col gap-4 text-center [--koyori-card-padding:1.5rem]"
    >
      <template v-if="token === ''">
        <h1 class="text-xl font-bold">リンクが正しくありません</h1>
        <p class="text-muted-foreground text-sm">招待メールのリンクをもう一度開いてください。</p>
      </template>
      <template v-else-if="previewQuery.isError.value">
        <h1 class="text-xl font-bold">
          {{ previewStatus === 410 ? '招待の期限が切れています' : 'この招待は使えません' }}
        </h1>
        <p class="text-muted-foreground text-sm" role="alert">
          <template v-if="previewStatus === 410">
            招待した人に送り直してもらってください。
          </template>
          <template v-else-if="previewStatus === 404">
            取り消されたか、使用済みか、新しい招待が送られています。最新の招待メールのリンクを開いてください。
          </template>
          <template v-else>招待を読み込めませんでした。時間をおいてやり直してください。</template>
        </p>
      </template>
      <p v-else-if="!preview" class="text-muted-foreground text-sm" role="status">読み込み中…</p>
      <template v-else>
        <h1 class="text-xl font-bold">{{ preview.tenant_name }} に参加しますか</h1>
        <p class="text-muted-foreground text-sm">
          {{ preview.invited_by }} さんが {{ preview.email }} を
          {{ preview.role }} として招待しました。
        </p>

        <template v-if="signedOut">
          <div class="flex flex-col gap-2">
            <Button type="button" label="サインインして参加" @click="signIn" />
            <a href="/signup" class="text-center text-sm underline underline-offset-4"
              >アカウントを作成</a
            >
          </div>
          <p class="text-muted-foreground text-xs">
            アカウントをお持ちでない場合は {{ preview.email }}
            で登録し、メールの確認を済ませてから、このリンクをもう一度開いてください。
          </p>
        </template>
        <p v-else-if="!meQuery.isSuccess.value" class="text-muted-foreground text-sm">
          読み込み中…
        </p>
        <template v-else-if="!addressMatches">
          <p class="text-sm" role="alert">
            いまは {{ meQuery.data.value!.email }} でログインしています。この招待は
            {{ preview.email }} 宛てなので、そのアドレスでログインし直してください。
          </p>
          <a href="/" class="text-sm underline underline-offset-4">ホームへ戻る</a>
        </template>
        <template v-else>
          <p v-if="acceptError" class="text-destructive text-sm" role="alert">
            {{ acceptError }}
          </p>
          <div class="flex flex-col gap-2">
            <Button
              type="button"
              :disabled="acceptMutation.isPending.value"
              :label="acceptMutation.isPending.value ? '参加しています…' : '参加する'"
              @click="accept"
            />
            <a href="/" class="text-center text-sm underline underline-offset-4">あとで</a>
          </div>
        </template>
      </template>
    </div>
  </div>
</template>
