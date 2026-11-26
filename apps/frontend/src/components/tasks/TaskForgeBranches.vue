<script setup lang="ts">
import { GitBranch, X } from '@lucide/vue';
import { useQueryClient } from '@tanstack/vue-query';
import { ref } from 'vue';

import { Button, HoverCard } from '@koyori-app/ui-vue';
import ForgeBranchCard from '@/components/tasks/ForgeBranchCard.vue';
import type { components } from '@/generated/api';
import { fetchClient } from '@/lib/api-vue-query';
import { httpUrl } from '@/lib/task-activity';

/**
 * リポジトリ欄の「ブランチ」タブの中身（docs/dev/features/tasks/github-tasks/index.md の「画面」）。
 * ブランチ名の `KEY-N` で結ばれたブランチを並べ、リンクを外せる。見出しはタブが持つ。
 */
const FORGE_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/forge' as const;
const BRANCH_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/forge/branches/{link_id}' as const;

const props = defineProps<{
  tenantId: string;
  projectId: string;
  taskId: string;
  branches: components['schemas']['TaskForgeBranch'][];
}>();

const queryClient = useQueryClient();
const removingId = ref<string | null>(null);
const errorMessage = ref<string | null>(null);

async function unlink(linkId: string) {
  if (removingId.value) return;
  removingId.value = linkId;
  errorMessage.value = null;
  const path = { tenant_id: props.tenantId, project_id: props.projectId, id: props.taskId };
  try {
    const { error, response } = await fetchClient.DELETE(BRANCH_PATH, {
      params: { path: { ...path, link_id: linkId } },
    });
    if (error) {
      throw new Error(
        response.status === 403
          ? 'ブランチのリンクを外す権限がありません'
          : 'ブランチのリンクを外せませんでした',
      );
    }
    await queryClient.invalidateQueries({
      queryKey: [
        'get',
        FORGE_PATH,
        { tenantId: props.tenantId, projectId: props.projectId, taskId: props.taskId },
      ],
    });
  } catch (error) {
    errorMessage.value =
      error instanceof Error ? error.message : 'ブランチのリンクを外せませんでした';
  } finally {
    removingId.value = null;
  }
}
</script>

<template>
  <div>
    <ul class="divide-y divide-border/60">
      <li
        v-for="branch in branches"
        :key="branch.id"
        class="flex min-h-12 items-center gap-2 py-1.5 text-sm"
        data-forge-branch
      >
        <GitBranch class="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
        <HoverCard :label="`${branch.branch} のブランチ`">
          <span class="min-w-0 flex-1">
            <a
              v-if="httpUrl(branch.html_url)"
              :href="httpUrl(branch.html_url)!"
              target="_blank"
              rel="noopener noreferrer"
              class="block truncate font-mono text-xs font-medium hover:underline"
              >{{ branch.branch }}</a
            >
            <span v-else class="block truncate font-mono text-xs font-medium">{{
              branch.branch
            }}</span>
          </span>
          <template #content><ForgeBranchCard :branch="branch" /></template>
        </HoverCard>
        <Button
          type="button"
          variant="ghost"
          class="ml-auto shrink-0"
          :ariaLabel="`${branch.branch} のリンクを外す`"
          :disabled="removingId === branch.id"
          @click="unlink(branch.id)"
        >
          <template #icon><X class="size-4" aria-hidden="true" /></template>
        </Button>
      </li>
    </ul>
    <p v-if="errorMessage" class="mt-1 text-sm text-destructive" role="alert">{{ errorMessage }}</p>
  </div>
</template>
