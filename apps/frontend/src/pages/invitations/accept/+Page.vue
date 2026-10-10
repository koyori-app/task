<script setup lang="ts">
import { usePageContext } from 'vike-vue/usePageContext';
import { computed, ref } from 'vue';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
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
    <Card class="w-full max-w-sm">
      <CardContent class="flex flex-col gap-4 p-6 text-center">
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
              <Button type="button" @click="signIn">サインインして参加</Button>
              <Button as="a" href="/signup" variant="outline">アカウントを作成</Button>
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
            <Button as="a" href="/" variant="outline">ホームへ戻る</Button>
          </template>
          <template v-else>
            <p v-if="acceptError" class="text-destructive text-sm" role="alert">
              {{ acceptError }}
            </p>
            <div class="flex flex-col gap-2">
              <Button type="button" :disabled="acceptMutation.isPending.value" @click="accept">
                {{ acceptMutation.isPending.value ? '参加しています…' : '参加する' }}
              </Button>
              <Button as="a" href="/" variant="outline">あとで</Button>
            </div>
          </template>
        </template>
      </CardContent>
    </Card>
  </div>
</template>
