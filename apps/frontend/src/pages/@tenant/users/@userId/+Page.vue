<script setup lang="ts">
import { Loader2 } from '@lucide/vue';
import { computed } from 'vue';
import { usePageContext } from 'vike-vue/usePageContext';

import UserAvatar from '@/components/UserAvatar.vue';
import { useResolvedTenantId } from '@/composables/useResolvedTenantId';
import { useMeQuery, useUserProfileQuery } from '@/lib/api-vue-query';

const pageContext = usePageContext();
const tenantDisplayId = computed(() => String(pageContext.routeParams.tenant ?? ''));
const userId = computed(() => String(pageContext.routeParams.userId ?? ''));

const { tenantId, isTenantNotFound, isResolving, isError } = useResolvedTenantId(tenantDisplayId);
const profileQuery = useUserProfileQuery(tenantId, userId);
const meQuery = useMeQuery();

const profile = computed(() => profileQuery.data.value);
const isForbidden = computed(
  () => (profileQuery.error.value as { status?: number } | null)?.status === 403,
);
</script>

<template>
  <div class="mx-auto flex w-full max-w-2xl flex-col gap-6 py-6">
    <div v-if="isResolving || profileQuery.isLoading.value" class="flex justify-center py-16">
      <Loader2 class="h-8 w-8 animate-spin text-muted-foreground" />
    </div>

    <p v-else-if="isError" class="py-16 text-center text-sm text-destructive">
      ページの読み込みに失敗しました
    </p>

    <p v-else-if="isTenantNotFound" class="py-16 text-center text-sm text-muted-foreground">
      テナントが見つかりません
    </p>

    <p v-else-if="isForbidden" class="py-16 text-center text-sm text-muted-foreground">
      このユーザーのプロフィールは閲覧できません
    </p>

    <p v-else-if="profileQuery.isError.value" class="py-16 text-center text-sm text-destructive">
      プロフィールの読み込みに失敗しました
    </p>

    <p v-else-if="profile === null" class="py-16 text-center text-sm text-muted-foreground">
      ユーザーが見つかりません
    </p>

    <section v-else-if="profile" class="flex flex-col gap-4">
      <header class="flex items-center gap-4">
        <UserAvatar :user="profile" size="3xl" shape="lg" />
        <div class="flex min-w-0 flex-1 flex-col gap-1">
          <h1 class="truncate text-2xl font-bold">{{ profile.username }}</h1>
          <a
            v-if="profile.id === meQuery.data.value?.id"
            href="/settings/profile"
            class="text-sm text-primary hover:underline"
          >
            プロフィールを編集
          </a>
        </div>
      </header>
      <p v-if="profile.bio" class="whitespace-pre-wrap break-words text-sm">{{ profile.bio }}</p>
      <p v-else class="text-sm text-muted-foreground">自己紹介はありません</p>
    </section>
  </div>
</template>
