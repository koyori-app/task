<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { usePageContext } from 'vike-vue/usePageContext';
import { useDefaultApi } from '@/composables/useDefaultApi';
import type { components } from '@/generated/api';

type Label = components['schemas']['crate.entities.labels.Model'];

const labels = ref<Label[]>([]);
const loading = ref(true);
const error = ref<string | null>(null);
const pageContext = usePageContext();

onMounted(async () => {
  try {
    loading.value = true;
    error.value = null;

    // projectKey is the route-level identifier for a project; assumed to equal
    // the project_id the API expects. If this system introduces separate slug
    // vs. UUID fields, resolve projectKey → project_id before this call.
    const { tenant, projectKey: projectId } = pageContext.routeParams;
    if (typeof tenant !== 'string' || typeof projectId !== 'string') {
      error.value = 'Missing route parameters';
      return;
    }

    const api = useDefaultApi();
    const { data, error: fetchError } = await api.GET(
      '/v1/tenants/{tenant_id}/projects/{project_id}/labels',
      { params: { path: { tenant_id: tenant, project_id: projectId } } },
    );
    if (fetchError) {
      error.value = 'Failed to fetch labels';
    } else {
      labels.value = data ?? [];
    }
  } catch (err) {
    console.error('Failed to fetch labels:', err);
    error.value = err instanceof Error ? err.message : 'Failed to fetch labels';
  } finally {
    loading.value = false;
  }
});
</script>

<template>
  <div class="rounded-lg border">
    <div class="p-6">
      <h2 class="text-2xl font-bold mb-4">Labels</h2>

      <!-- Loading State -->
      <div v-if="loading" class="flex items-center justify-center h-32">
        <div class="text-center">
          <div
            class="animate-spin rounded-full h-8 w-8 border-b-2 border-gray-900 mx-auto mb-2"
          ></div>
          <p class="text-gray-600">Loading labels...</p>
        </div>
      </div>

      <!-- Error State -->
      <div v-else-if="error" class="p-4 bg-red-50 border border-red-200 rounded-md">
        <p class="text-red-800">Error: {{ error }}</p>
      </div>

      <!-- Empty State -->
      <div v-else-if="labels.length === 0" class="text-center py-8">
        <p class="text-gray-500">No labels found</p>
      </div>

      <!-- Labels Table -->
      <div v-else class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead>
            <tr class="border-b">
              <th class="text-left py-3 px-4 font-semibold">Color</th>
              <th class="text-left py-3 px-4 font-semibold">Name</th>
              <th class="text-left py-3 px-4 font-semibold">Description</th>
              <th class="text-left py-3 px-4 font-semibold">Icon</th>
              <th class="text-left py-3 px-4 font-semibold">ID</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="label in labels" :key="label.id" class="border-b hover:bg-gray-50">
              <td class="py-3 px-4">
                <div class="flex items-center gap-2">
                  <div class="w-6 h-6 rounded border" :style="{ backgroundColor: label.color }" />
                  <span class="text-xs text-gray-500">{{ label.color }}</span>
                </div>
              </td>
              <td class="py-3 px-4 font-medium">{{ label.name }}</td>
              <td class="py-3 px-4 text-gray-600">{{ label.description }}</td>
              <td class="py-3 px-4">
                <a
                  v-if="label.icon_url"
                  :href="label.icon_url"
                  target="_blank"
                  rel="noopener noreferrer"
                  class="text-blue-600 hover:underline text-xs"
                >
                  View Icon
                </a>
                <span v-else class="text-gray-400">-</span>
              </td>
              <td class="py-3 px-4 text-xs text-gray-500 font-mono">{{ label.id }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>
