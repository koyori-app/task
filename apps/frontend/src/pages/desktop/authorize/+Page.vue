<script setup lang="ts">
import { usePageContext } from 'vike-vue/usePageContext';
import { ref, watch } from 'vue';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { useCreateDesktopAuthCodeMutation, useMeQuery } from '@/lib/api-vue-query';
import {
  DESKTOP_AUTHORIZE_RETURN,
  desktopCallbackUrl,
  parseDesktopAuthorizeQuery,
} from '@/lib/desktop-authorize';
import { markNotice } from '@/lib/one-time-notice';

// 認証ガードの外に置き（+Layout の isAuthPage）、未ログインならこの URL を覚えて
// サインインへ送る。ガードに任せるとクエリが落ちて Desktop からやり直しになる
const pageContext = usePageContext();
const request = parseDesktopAuthorizeQuery(
  new URLSearchParams(
    (pageContext as { urlParsed?: { search?: Record<string, string> } }).urlParsed?.search ?? {},
  ),
);

const meQuery = useMeQuery();
const createCode = useCreateDesktopAuthCodeMutation();
const submitError = ref<string | null>(null);
const done = ref(false);

watch(
  () => meQuery.isError.value && !meQuery.isFetching.value,
  (signedOut) => {
    if (!signedOut || !request) return;
    markNotice(DESKTOP_AUTHORIZE_RETURN, `${window.location.pathname}${window.location.search}`);
    window.location.assign('/signin');
  },
  { immediate: true },
);

async function approve() {
  if (!request) return;
  submitError.value = null;
  try {
    const { code } = await createCode.mutateAsync({
      body: { code_challenge: request.codeChallenge, name: request.name },
    });
    done.value = true;
    window.location.assign(desktopCallbackUrl(request.port, code, request.state));
  } catch {
    submitError.value = '承認できませんでした。Koyori Desktop からやり直してください。';
  }
}
</script>

<template>
  <div class="bg-muted flex min-h-svh flex-col items-center justify-center p-6 md:p-10">
    <Card class="w-full max-w-sm">
      <CardContent class="flex flex-col gap-4 p-6 text-center">
        <template v-if="!request">
          <h1 class="text-xl font-bold">リンクが正しくありません</h1>
          <p class="text-muted-foreground text-sm">Koyori Desktop からやり直してください。</p>
        </template>
        <p v-else-if="!meQuery.isSuccess.value" class="text-muted-foreground text-sm">
          読み込み中…
        </p>
        <p v-else-if="done" class="text-muted-foreground text-sm">
          Koyori Desktop に戻っています。このタブは閉じてかまいません。
        </p>
        <template v-else>
          <h1 class="text-xl font-bold">Koyori Desktop（{{ request.name }}）を承認しますか</h1>
          <p class="text-muted-foreground text-sm">
            {{ meQuery.data.value!.username }}
            としてサインインします。承認した端末はアカウント設定のセキュリティから失効できます。
          </p>
          <p v-if="submitError" class="text-destructive text-sm">{{ submitError }}</p>
          <div class="flex flex-col gap-2">
            <Button type="button" :disabled="createCode.isPending.value" @click="approve">
              {{ createCode.isPending.value ? '承認中…' : '承認する' }}
            </Button>
            <Button as="a" href="/" variant="outline">キャンセル</Button>
          </div>
        </template>
      </CardContent>
    </Card>
  </div>
</template>
