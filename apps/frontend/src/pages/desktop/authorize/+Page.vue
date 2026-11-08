<script setup lang="ts">
import { usePageContext } from 'vike-vue/usePageContext';
import { computed, ref, watch } from 'vue';
import { Button } from '@koyori-app/ui-vue';
import {
  isSessionRejected,
  useCreateDesktopAuthCodeMutation,
  useMeQuery,
} from '@/lib/api-vue-query';
import { currentLocationPath, signInUrl } from '@/composables/useRedirectAfter';
import { desktopCallbackUrl, parseDesktopAuthorizeQuery } from '@/lib/desktop-authorize';

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

const meLoadFailed = computed(
  () => meQuery.isError.value && !meQuery.data.value && !isSessionRejected(meQuery.error.value),
);

watch(
  () => isSessionRejected(meQuery.error.value) && !meQuery.isFetching.value,
  (signedOut) => {
    if (!signedOut || !request) return;
    window.location.assign(signInUrl(currentLocationPath()));
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
    <div
      class="koyori-card flex w-full max-w-sm flex-col gap-4 text-center [--koyori-card-padding:1.5rem]"
    >
      <template v-if="!request">
        <h1 class="text-xl font-bold">リンクが正しくありません</h1>
        <p class="text-muted-foreground text-sm">Koyori Desktop からやり直してください。</p>
      </template>
      <p v-else-if="meLoadFailed" class="text-sm" role="alert">
        サーバーに接続できませんでした。時間をおいてやり直してください。
      </p>
      <p v-else-if="!meQuery.data.value" class="text-muted-foreground text-sm">読み込み中…</p>
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
          <Button
            type="button"
            :disabled="createCode.isPending.value"
            :label="createCode.isPending.value ? '承認中…' : '承認する'"
            @click="approve"
          />
          <a href="/" class="text-center text-sm underline underline-offset-4">キャンセル</a>
        </div>
      </template>
    </div>
  </div>
</template>
