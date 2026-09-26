<script setup lang="ts">
import { useQuery, useQueryClient } from '@tanstack/vue-query';
import { PhPlus } from '@phosphor-icons/vue';
import { computed, ref } from 'vue';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Skeleton } from '@/components/ui/skeleton';
import WebhookFormDialog from '@/components/projects/WebhookFormDialog.vue';
import {
  webhookErrorMessage,
  webhookEventLabel,
  webhookFormatIcon,
  webhookFormatLabel,
} from '@/components/projects/webhook-events';
import { apiClient } from '@/lib/api-vue-query';
import type { components } from '@/generated/api';

type WebhookResponse = components['schemas']['WebhookResponse'];
type WebhookDeliveryResponse = components['schemas']['WebhookDeliveryResponse'];

const WEBHOOKS_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/webhooks' as const;
const WEBHOOK_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/webhooks/{id}' as const;
const DELIVERIES_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/webhooks/{id}/deliveries' as const;
const REDELIVER_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/webhooks/{id}/deliveries/{delivery_id}/redeliver' as const;

/** 打ち止めがこれだけ続くと backend が Webhook を止める（仕様 §5.1） */
const MAX_FAILURE_STREAK = 5;
const DELIVERY_LIMIT = 20;

const props = defineProps<{
  tenantId: string;
  projectId: string;
}>();

const queryClient = useQueryClient();
const isFormOpen = ref(false);
const editingWebhook = ref<WebhookResponse | null>(null);
const deleteTarget = ref<WebhookResponse | null>(null);
const deleteError = ref<string | null>(null);
const actionError = ref<string | null>(null);
/** 配信履歴を開いている Webhook（一度に 1 つ） */
const expandedId = ref<string | null>(null);

// options は computed にして props に追従させる（プロジェクト切り替えで前の一覧を残さない）
const webhooksQuery = useQuery(
  computed(() => ({
    ...apiClient.queryOptions('get', WEBHOOKS_PATH, {
      params: { path: { tenant_id: props.tenantId, project_id: props.projectId } },
    }),
    retry: false,
  })),
);
const webhooks = computed<WebhookResponse[]>(() => webhooksQuery.data.value ?? []);
const listError = computed(() =>
  webhooksQuery.isError.value
    ? webhookErrorMessage(webhooksQuery.error.value, 'Webhook を読み込めませんでした')
    : null,
);

const deliveriesQuery = useQuery(
  computed(() => ({
    ...apiClient.queryOptions('get', DELIVERIES_PATH, {
      params: {
        path: {
          tenant_id: props.tenantId,
          project_id: props.projectId,
          id: expandedId.value ?? '',
        },
        query: { limit: DELIVERY_LIMIT },
      },
    }),
    enabled: expandedId.value !== null,
    retry: false,
  })),
);
const deliveries = computed<WebhookDeliveryResponse[]>(() => deliveriesQuery.data.value ?? []);

const updateMutation = apiClient.useMutation('put', WEBHOOK_PATH);
const deleteMutation = apiClient.useMutation('delete', WEBHOOK_PATH);
const redeliverMutation = apiClient.useMutation('post', REDELIVER_PATH);

function isStopped(webhook: WebhookResponse) {
  return !webhook.is_active && webhook.failure_streak >= MAX_FAILURE_STREAK;
}

function formatDateTime(value: string) {
  return new Date(value).toLocaleString('ja-JP');
}

function formatTime(value: string) {
  return new Date(value).toLocaleTimeString('ja-JP', { hour: '2-digit', minute: '2-digit' });
}

function deliveryResult(delivery: WebhookDeliveryResponse) {
  if (delivery.delivered_at) return `成功 (HTTP ${delivery.status_code ?? '-'})`;
  if (delivery.next_attempt_at) {
    return delivery.attempt === 0
      ? '送信待ち'
      : `再試行待ち (${delivery.attempt + 1} 回目、次回 ${formatTime(delivery.next_attempt_at)})`;
  }
  return `失敗 (${delivery.attempt} 回)`;
}

function openCreate() {
  editingWebhook.value = null;
  isFormOpen.value = true;
}

function openEdit(webhook: WebhookResponse) {
  editingWebhook.value = webhook;
  isFormOpen.value = true;
}

function toggleDeliveries(webhook: WebhookResponse) {
  expandedId.value = expandedId.value === webhook.id ? null : webhook.id;
}

async function toggleActive(webhook: WebhookResponse) {
  actionError.value = null;
  try {
    await updateMutation.mutateAsync({
      params: {
        path: { tenant_id: props.tenantId, project_id: props.projectId, id: webhook.id },
      },
      body: { is_active: !webhook.is_active },
    });
    await queryClient.invalidateQueries({ queryKey: ['get', WEBHOOKS_PATH] });
  } catch (e) {
    actionError.value = webhookErrorMessage(e, 'Webhook を更新できませんでした');
  }
}

async function redeliver(webhookId: string, delivery: WebhookDeliveryResponse) {
  actionError.value = null;
  try {
    await redeliverMutation.mutateAsync({
      params: {
        path: {
          tenant_id: props.tenantId,
          project_id: props.projectId,
          id: webhookId,
          delivery_id: delivery.id,
        },
      },
    });
    await queryClient.invalidateQueries({ queryKey: ['get', DELIVERIES_PATH] });
  } catch (e) {
    actionError.value = webhookErrorMessage(e, '再送できませんでした');
  }
}

function openDelete(webhook: WebhookResponse) {
  deleteError.value = null;
  deleteTarget.value = webhook;
}

function onDeleteOpenChange(open: boolean) {
  // 削除リクエスト進行中はダイアログを閉じない（結果の見逃し防止）
  if (!open && deleteMutation.isPending.value) return;
  if (!open) deleteTarget.value = null;
}

async function confirmDelete() {
  const target = deleteTarget.value;
  if (!target) return;
  deleteError.value = null;
  try {
    await deleteMutation.mutateAsync({
      params: {
        path: { tenant_id: props.tenantId, project_id: props.projectId, id: target.id },
      },
    });
    await queryClient.invalidateQueries({ queryKey: ['get', WEBHOOKS_PATH] });
    if (expandedId.value === target.id) expandedId.value = null;
    deleteTarget.value = null;
  } catch (e) {
    deleteError.value = webhookErrorMessage(e, 'Webhook を削除できませんでした');
  }
}
</script>

<template>
  <div>
    <div class="mb-5 flex items-start justify-between gap-4 border-b pb-4">
      <div>
        <h2 class="text-xl font-semibold">Webhook</h2>
        <p class="mt-1 text-sm text-muted-foreground">
          タスク・コメント・レビューのイベントを外部の URL へ送ります
        </p>
      </div>
      <Button type="button" variant="outline" class="shrink-0" @click="openCreate">
        <PhPlus class="size-4" />
        追加
      </Button>
    </div>

    <p v-if="actionError" role="alert" class="mb-4 text-sm text-destructive">{{ actionError }}</p>

    <div v-if="webhooksQuery.isPending.value" class="flex flex-col gap-2">
      <Skeleton class="h-16 w-full" />
      <Skeleton class="h-16 w-full" />
    </div>

    <p v-else-if="listError" role="alert" class="text-sm text-destructive">{{ listError }}</p>

    <p v-else-if="webhooks.length === 0" class="text-sm text-muted-foreground">
      Webhook はまだありません
    </p>

    <ul v-else class="overflow-hidden rounded-lg border">
      <li
        v-for="webhook in webhooks"
        :key="webhook.id"
        class="border-b px-3.5 py-3 last:border-b-0"
        data-testid="webhook-row"
      >
        <div class="flex flex-wrap items-center gap-2">
          <span class="min-w-0 flex-1 truncate font-mono text-sm" :title="webhook.url">
            {{ webhook.url }}
          </span>
          <span
            class="inline-flex shrink-0 items-center rounded-full border p-1 text-muted-foreground"
            :title="webhookFormatLabel(webhook.format)"
          >
            <component :is="webhookFormatIcon(webhook.format)" class="size-4" aria-hidden="true" />
            <span class="sr-only">{{ webhookFormatLabel(webhook.format) }}</span>
          </span>
        </div>

        <div class="mt-1.5 flex flex-wrap items-center gap-1.5 text-xs">
          <span
            v-for="event in webhook.events"
            :key="event"
            class="rounded bg-muted px-1.5 py-0.5 text-muted-foreground"
          >
            {{ webhookEventLabel(event) }}
          </span>
        </div>

        <div class="mt-2 flex flex-wrap items-center gap-2">
          <span class="text-xs" :class="webhook.is_active ? '' : 'text-muted-foreground'">
            {{ webhook.is_active ? '有効' : '無効' }}
          </span>
          <span v-if="isStopped(webhook)" class="text-xs text-amber-600 dark:text-amber-500">
            連続失敗で停止
          </span>
          <span
            v-else-if="webhook.failure_streak > 0"
            class="text-xs text-amber-600 dark:text-amber-500"
          >
            連続失敗 {{ webhook.failure_streak }} 回
          </span>
          <div class="ml-auto flex flex-wrap gap-1">
            <Button
              type="button"
              variant="ghost"
              size="sm"
              :aria-expanded="expandedId === webhook.id"
              @click="toggleDeliveries(webhook)"
            >
              配信履歴
            </Button>
            <Button type="button" variant="ghost" size="sm" @click="openEdit(webhook)">
              編集
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              :disabled="updateMutation.isPending.value"
              @click="toggleActive(webhook)"
            >
              {{ webhook.is_active ? '無効にする' : '有効にする' }}
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              class="text-destructive hover:text-destructive"
              @click="openDelete(webhook)"
            >
              削除
            </Button>
          </div>
        </div>

        <!-- 配信履歴 -->
        <div v-if="expandedId === webhook.id" class="mt-3 rounded-md border bg-muted/30 p-3">
          <Skeleton v-if="deliveriesQuery.isPending.value" class="h-10 w-full" />
          <p
            v-else-if="deliveriesQuery.isError.value"
            role="alert"
            class="text-sm text-destructive"
          >
            配信履歴を読み込めませんでした
          </p>
          <p v-else-if="deliveries.length === 0" class="text-sm text-muted-foreground">
            配信履歴はまだありません
          </p>
          <ul v-else class="flex flex-col gap-2" data-testid="delivery-list">
            <li
              v-for="delivery in deliveries"
              :key="delivery.id"
              class="border-b pb-2 text-xs last:border-b-0 last:pb-0"
            >
              <div class="flex flex-wrap items-center gap-2">
                <span class="text-muted-foreground">{{ formatDateTime(delivery.created_at) }}</span>
                <span>{{ webhookEventLabel(delivery.event) }}</span>
                <span
                  :class="
                    delivery.delivered_at || delivery.next_attempt_at ? '' : 'text-destructive'
                  "
                  >{{ deliveryResult(delivery) }}</span
                >
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  class="ml-auto h-7"
                  :disabled="redeliverMutation.isPending.value"
                  @click="redeliver(webhook.id, delivery)"
                >
                  再送
                </Button>
              </div>
              <p
                v-if="!delivery.delivered_at && !delivery.next_attempt_at && delivery.last_error"
                class="mt-1 break-all text-destructive"
              >
                {{ delivery.last_error }}
              </p>
              <details class="mt-1">
                <summary class="cursor-pointer text-muted-foreground">payload</summary>
                <pre class="mt-1 overflow-x-auto rounded bg-muted p-2 font-mono">{{
                  JSON.stringify(delivery.payload, null, 2)
                }}</pre>
              </details>
            </li>
          </ul>
        </div>
      </li>
    </ul>

    <WebhookFormDialog
      v-if="isFormOpen"
      :tenant-id="tenantId"
      :project-id="projectId"
      :webhook="editingWebhook"
      @close="isFormOpen = false"
    />

    <Dialog v-if="deleteTarget" :open="true" @update:open="onDeleteOpenChange">
      <DialogContent class="max-w-md" :show-close-button="false">
        <DialogHeader>
          <DialogTitle>Webhook を削除しますか？</DialogTitle>
          <DialogDescription>
            <span class="break-all font-mono">{{ deleteTarget.url }}</span>
            への送信をやめ、配信履歴も削除します。この操作は取り消せません。
          </DialogDescription>
        </DialogHeader>
        <p v-if="deleteError" role="alert" class="text-sm text-destructive">{{ deleteError }}</p>
        <DialogFooter>
          <Button
            type="button"
            variant="outline"
            :disabled="deleteMutation.isPending.value"
            @click="deleteTarget = null"
          >
            キャンセル
          </Button>
          <Button
            type="button"
            variant="destructive"
            :disabled="deleteMutation.isPending.value"
            @click="confirmDelete"
          >
            {{ deleteMutation.isPending.value ? '削除中…' : '削除する' }}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </div>
</template>
