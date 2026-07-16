<script setup lang="ts">
import { Loader2 } from '@lucide/vue';
import { useQuery } from '@tanstack/vue-query';
import { computed } from 'vue';
import { usePageContext } from 'vike-vue/usePageContext';

import { useResolvedProjectId } from '@/composables/useResolvedProjectId';
import { useResolvedTenantId } from '@/composables/useResolvedTenantId';
import { projectLabelsQueryOptions } from '@/lib/api-vue-query';

const pageContext = usePageContext();
const tenantDisplayId = computed(() => String(pageContext.routeParams.tenant ?? ''));
const {
  tenantId,
  isTenantNotFound,
  isResolving: isTenantResolving,
  isError: isTenantResolveError,
} = useResolvedTenantId(tenantDisplayId);
const projectKey = computed(() => String(pageContext.routeParams.projectKey ?? ''));
const {
  projectId,
  isProjectNotFound,
  isResolving: isProjectResolving,
  isError: isProjectResolveError,
} = useResolvedProjectId(tenantId, projectKey);

const labelsQuery = useQuery(
  computed(() => ({
    ...projectLabelsQueryOptions(tenantId.value, projectId.value),
    enabled: !!tenantId.value && !!projectId.value,
  })),
);

const labels = computed(() => labelsQuery.data.value ?? []);
const isInitialLoading = computed(
  () => isTenantResolving.value || isProjectResolving.value || labelsQuery.isLoading.value,
);
const isError = computed(
  () => isTenantResolveError.value || isProjectResolveError.value || labelsQuery.isError.value,
);
</script>

<template>
  <div class="rounded-lg border">
    <div class="p-6">
      <h2 class="mb-4 text-2xl font-bold">Labels</h2>

      <div v-if="isInitialLoading" class="flex h-32 items-center justify-center">
        <div class="text-center">
          <Loader2 class="mx-auto mb-2 h-8 w-8 animate-spin text-muted-foreground" />
          <p class="text-muted-foreground">Loading labels...</p>
        </div>
      </div>

      <div
        v-else-if="isError"
        class="rounded-md border border-destructive/30 bg-destructive/10 p-4"
      >
        <p class="text-destructive">Failed to fetch labels</p>
      </div>

      <div v-else-if="isTenantNotFound" class="py-8 text-center">
        <p class="text-muted-foreground">Tenant not found</p>
      </div>

      <div v-else-if="isProjectNotFound" class="py-8 text-center">
        <p class="text-muted-foreground">Project not found</p>
      </div>

      <div v-else-if="labels.length === 0" class="py-8 text-center">
        <p class="text-muted-foreground">No labels found</p>
      </div>

      <div v-else class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead>
            <tr class="border-b">
              <th class="px-4 py-3 text-left font-semibold">Color</th>
              <th class="px-4 py-3 text-left font-semibold">Name</th>
              <th class="px-4 py-3 text-left font-semibold">Description</th>
              <th class="px-4 py-3 text-left font-semibold">Icon</th>
              <th class="px-4 py-3 text-left font-semibold">ID</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="label in labels" :key="label.id" class="border-b hover:bg-muted/50">
              <td class="px-4 py-3">
                <div class="flex items-center gap-2">
                  <div class="h-6 w-6 rounded border" :style="{ backgroundColor: label.color }" />
                  <span class="text-xs text-muted-foreground">{{ label.color }}</span>
                </div>
              </td>
              <td class="px-4 py-3 font-medium">{{ label.name }}</td>
              <td class="px-4 py-3 text-muted-foreground">{{ label.description }}</td>
              <td class="px-4 py-3">
                <a
                  v-if="label.icon_url"
                  :href="label.icon_url"
                  target="_blank"
                  rel="noopener noreferrer"
                  class="text-xs text-primary hover:underline"
                >
                  View Icon
                </a>
                <span v-else class="text-muted-foreground">-</span>
              </td>
              <td class="px-4 py-3 font-mono text-xs text-muted-foreground">{{ label.id }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>
