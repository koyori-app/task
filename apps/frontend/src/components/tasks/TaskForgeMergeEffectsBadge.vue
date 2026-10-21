<script setup lang="ts">
import { useQueryClient } from '@tanstack/vue-query';
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
import { ACTIVITIES_PATH } from '@/composables/useTaskActivities';
import type { components } from '@/generated/api';
import { apiClient } from '@/lib/api-vue-query';

/**
 * マージ済みなのに自動完了を行っていない PR の印と、確認済みにする操作
 * （docs/features/tasks/9.github-tasks.md §9）。
 *
 * マージ時のイベントを受け取れていない PR だけに出る。現在のリンクはマージ後の編集で
 * 変わりうるので、ここから一括で完了させる操作は置かない。
 */
const FORGE_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/forge' as const;
const ACKNOWLEDGE_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/forge/pull-requests/{pull_request_id}/merge-effects/acknowledge' as const;

const props = defineProps<{
  tenantId: string;
  pullRequest: components['schemas']['TaskForgePullRequest'];
}>();

const pending = computed(
  () => props.pullRequest.state === 'merged' && !props.pullRequest.merge_effects_processed_at,
);
const open = ref(false);
const confirming = ref(false);
const error = ref<string | null>(null);

const queryClient = useQueryClient();
const acknowledgeMutation = apiClient.useMutation('post', ACKNOWLEDGE_PATH);

function openDialog() {
  confirming.value = false;
  error.value = null;
  open.value = true;
}

function onOpenChange(next: boolean) {
  // 送信中に閉じると結果を見逃す
  if (!next && acknowledgeMutation.isPending.value) return;
  open.value = next;
}

async function acknowledge() {
  error.value = null;
  try {
    await acknowledgeMutation.mutateAsync({
      params: {
        path: {
          tenant_id: props.tenantId,
          // PR を主語にする口なので、タスクではなく PR 行のプロジェクトを使う
          project_id: props.pullRequest.project_id,
          pull_request_id: props.pullRequest.id,
        },
      },
    });
    open.value = false;
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ['get', FORGE_PATH] }),
      queryClient.invalidateQueries({ queryKey: ['get', ACTIVITIES_PATH] }),
    ]);
  } catch {
    error.value = '確認済みにできませんでした';
  }
}
</script>

<template>
  <template v-if="pending">
    <button
      type="button"
      class="shrink-0 rounded border border-amber-500/50 px-1 text-xs text-amber-700 hover:bg-amber-500/10 dark:text-amber-400"
      data-merge-effects-badge
      @click="openDialog"
    >
      マージ済み（自動完了は未適用）
    </button>
    <Dialog v-if="open" :open="true" @update:open="onOpenChange">
      <DialogContent class="max-w-md">
        <DialogHeader>
          <DialogTitle>#{{ pullRequest.number }} の自動完了は未適用です</DialogTitle>
          <DialogDescription>
            この PR
            はマージ済みですが、マージ時のイベントを受け取れていないため、リンク先タスクの自動完了を行っていません。
          </DialogDescription>
        </DialogHeader>
        <ul class="list-disc space-y-1 pl-5 text-sm">
          <li>完了すべきタスクは、各タスクのステータスを手で変更してください。</li>
          <li>
            連携の管理者に元のマージ（closed）の Webhook
            を再配送してもらうと、通常どおり自動完了します。
          </li>
          <li>対応を終えたら確認済みにすると、この表示が消えます。タスクは完了しません。</li>
        </ul>
        <p v-if="confirming" class="text-sm font-medium" data-merge-effects-confirm>
          自動完了を未適用のまま確認済みにしますか？後から元の Webhook が届いても自動完了しません。
        </p>
        <p v-if="error" role="alert" class="text-sm text-destructive">{{ error }}</p>
        <DialogFooter>
          <template v-if="confirming">
            <Button
              type="button"
              variant="outline"
              :disabled="acknowledgeMutation.isPending.value"
              @click="confirming = false"
            >
              キャンセル
            </Button>
            <Button
              type="button"
              :disabled="acknowledgeMutation.isPending.value"
              @click="acknowledge"
            >
              {{ acknowledgeMutation.isPending.value ? '送信中…' : '確認済みにする' }}
            </Button>
          </template>
          <template v-else>
            <Button type="button" variant="outline" @click="open = false">閉じる</Button>
            <Button type="button" @click="confirming = true">確認済みにする…</Button>
          </template>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </template>
</template>
