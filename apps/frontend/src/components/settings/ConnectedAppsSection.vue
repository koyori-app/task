<script setup lang="ts">
import { useQueryClient } from '@tanstack/vue-query';
import { PhPlugs } from '@phosphor-icons/vue';
import { computed, ref } from 'vue';
import { Button, Dialog, Dropdown, Skeleton, type DropdownItem } from '@koyori-app/ui-vue';
import {
  oauthGrantsQueryOptions,
  useOAuthGrantsQuery,
  useRevokeOAuthGrantMutation,
  useTenantsQuery,
} from '@/lib/api-vue-query';
import type { OAuthGrant } from '@/lib/oauth-authorize';
import { formatExpiry, formatLastUsed, formatTenantNames } from '@/lib/personal-tokens';

const queryClient = useQueryClient();
const grantsQuery = useOAuthGrantsQuery();
const tenantsQuery = useTenantsQuery();
const revokeGrant = useRevokeOAuthGrantMutation();

const tenantNamesById = computed(
  () => new Map((tenantsQuery.data.value ?? []).map((t) => [t.id, t.name])),
);

function tenantName(tenantId: string) {
  return tenantNamesById.value.get(tenantId) ?? '不明なテナント';
}

/** 名前はアプリの自己申告なので、`client_id` の host を並べて出す。 */
function clientHost(clientId: string) {
  try {
    return new URL(clientId).host;
  } catch {
    return clientId;
  }
}

/** スコープは見せるだけ。行ごとに出すので、配列は行の scopes から組む */
function scopeItems(scopes: string[]): DropdownItem[] {
  return scopes.map((scope) => ({ value: scope, label: scope, disabled: true }));
}

const revokeTarget = ref<OAuthGrant | null>(null);
const revokeError = ref<string | null>(null);

const revokeDescription = computed(() =>
  revokeTarget.value
    ? `「${revokeTarget.value.client_name}」（${clientHost(revokeTarget.value.client_id)}）の、テナント「${revokeTarget.value.tenant_ids.map(tenantName).join('、')}」への接続を解除します。このアプリはすぐに接続できなくなります。もう一度使うには、アプリから接続し直してください。`
    : '',
);

function onRevokeClose() {
  // 解除の要求が進んでいる間は閉じない（結果の見逃しを防ぐ）
  if (!revokeGrant.isPending.value) revokeTarget.value = null;
}

async function onRevokeConfirm() {
  if (!revokeTarget.value) return;
  revokeError.value = null;
  try {
    await revokeGrant.mutateAsync({ params: { path: { id: revokeTarget.value.id } } });
    await queryClient.invalidateQueries({ queryKey: oauthGrantsQueryOptions().queryKey });
    revokeTarget.value = null;
  } catch {
    revokeError.value = '接続を解除できませんでした。時間をおいて再度お試しください。';
  }
}
</script>

<template>
  <div class="flex flex-col gap-4">
    <div class="flex flex-col gap-1">
      <h2 class="text-lg font-semibold">連携中のアプリ</h2>
      <p class="text-muted-foreground text-sm">
        MCP で接続を許可したアプリです。接続を解除すると、そのアプリはあなたとして動けなくなります。
      </p>
    </div>

    <Skeleton v-if="grantsQuery.isPending.value" :lines="2" height="4rem" />
    <p v-else-if="grantsQuery.isError.value" class="text-destructive text-sm">
      連携中のアプリの一覧を取得できませんでした。再読み込みしてください。
    </p>
    <p v-else-if="grantsQuery.data.value!.length === 0" class="text-muted-foreground text-sm">
      連携中のアプリはありません。
    </p>
    <ul v-else class="divide-y rounded-lg border" data-testid="connected-app-list">
      <li
        v-for="grant in grantsQuery.data.value"
        :key="grant.id"
        class="flex items-center gap-4 p-4"
      >
        <div
          class="bg-muted text-muted-foreground hidden size-10 shrink-0 place-content-center rounded-full sm:grid"
        >
          <PhPlugs class="size-5" />
        </div>
        <div class="flex min-w-0 flex-1 flex-col gap-0.5">
          <p class="truncate text-sm font-medium">{{ grant.client_name }}</p>
          <p class="text-muted-foreground flex flex-wrap items-center gap-x-2 text-xs">
            <code class="font-mono">{{ clientHost(grant.client_id) }}</code>
            <span>·</span>
            <span>{{ formatTenantNames(grant.tenant_ids.map(tenantName)) }}</span>
            <span>·</span>
            <span>{{ grant.scopes.length }} スコープ</span>
            <span>·</span>
            <span>{{ formatExpiry(grant.expires_at) }}</span>
          </p>
          <p class="text-muted-foreground text-xs">{{ formatLastUsed(grant.last_used_at) }}</p>
        </div>
        <Dropdown variant="tertiary" label="スコープを表示" :items="scopeItems(grant.scopes)" />
        <Button
          type="button"
          variant="tertiary"
          class="text-(--koyori-color-danger)"
          label="接続を解除"
          @click="
            revokeTarget = grant;
            revokeError = null;
          "
        />
      </li>
    </ul>

    <!-- 解除の確認 -->
    <Dialog
      :open="revokeTarget !== null"
      title="接続を解除しますか？"
      :description="revokeDescription"
      :on-close="onRevokeClose"
    >
      <p v-if="revokeError" class="text-destructive text-sm">{{ revokeError }}</p>
      <template #actions>
        <Button
          type="button"
          variant="tertiary"
          :disabled="revokeGrant.isPending.value"
          label="キャンセル"
          @click="revokeTarget = null"
        />
        <Button
          type="button"
          variant="danger"
          :disabled="revokeGrant.isPending.value"
          :label="revokeGrant.isPending.value ? '解除中…' : '解除する'"
          @click="onRevokeConfirm"
        />
      </template>
    </Dialog>
  </div>
</template>
