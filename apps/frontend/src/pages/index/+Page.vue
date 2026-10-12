<script setup lang="ts">
import { ref, watch } from 'vue';
import { navigate } from 'vike/client/router';
import { useTenantsQuery } from '@/lib/api-vue-query';
import { useTenantStore } from '@/stores/tenant';
import { useHydrated } from '@/composables/useHydrated';
import CreateTenantDialog from '@/components/header/CreateTenantDialog.vue';
import { Button } from '@/components/ui/button';

const store = useTenantStore();
const query = useTenantsQuery();
const hydrated = useHydrated();
const createOpen = ref(false);
const navigationError = ref(false);
async function openHome() {
  if (!hydrated.value || !query.isSuccess.value || !query.data.value?.length) return;
  navigationError.value = false;
  const tenant =
    query.data.value.find((t) => t.id === store.selectedTenantId) ?? query.data.value[0]!;
  store.selectTenant(tenant);
  try {
    await navigate(`/${tenant.display_id}`, { overwriteLastHistoryEntry: true });
  } catch {
    navigationError.value = true;
  }
}
watch([hydrated, query.data], () => void openHome(), { immediate: true });
</script>

<template>
  <div class="mx-auto flex max-w-lg flex-col items-center gap-4 py-20 text-center">
    <template v-if="query.isError.value || navigationError">
      <p role="alert" class="text-sm text-destructive">ホームを開けませんでした。</p>
      <Button variant="outline" @click="navigationError ? openHome() : query.refetch()"
        >再試行</Button
      >
    </template>
    <template v-else-if="query.isSuccess.value && !query.data.value?.length">
      <h1 class="text-2xl font-semibold">Koyori へようこそ</h1>
      <p class="text-sm text-muted-foreground">
        ワークスペースを作成して、タスクをまとめましょう。
      </p>
      <Button @click="createOpen = true">ワークスペースを作成</Button>
      <CreateTenantDialog v-model:open="createOpen" />
    </template>
    <p v-else role="status" class="text-sm text-muted-foreground">ホームを開いています…</p>
  </div>
</template>
