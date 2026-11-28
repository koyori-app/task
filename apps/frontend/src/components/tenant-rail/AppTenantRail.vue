<script setup lang="ts">
import { ContextMenu } from '@koyori-app/ui-vue';
import { usePageContext } from 'vike-vue/usePageContext';
import { computed, ref, watch } from 'vue';

import CreateTenantDialog from '@/components/tenant-rail/CreateTenantDialog.vue';
import { useTenantMenu } from '@/composables/useTenantMenu';
import { useTenantStore, type Tenant } from '@/stores/tenant';

const emit = defineEmits<{
  /** 切り替え・遷移・ダイアログを開く前に出す。狭い画面では Drawer を閉じる */
  close: [];
}>();

const pageContext = usePageContext();
const tenantStore = useTenantStore();

const tenantSlug = computed(() => {
  const { tenant } = pageContext.routeParams;
  return typeof tenant === 'string' ? tenant : '';
});

watch(tenantSlug, (slug) => void tenantStore.loadTenants(slug || undefined), { immediate: true });

function selectTenant(tenant: Tenant) {
  emit('close');
  tenantStore.selectTenant(tenant);
  if (tenant.display_id !== tenantSlug.value) {
    // テナントに紐づく状態を残さないためフルページ遷移にする
    window.location.assign(`/${tenant.display_id}`);
  }
}

function retry() {
  void tenantStore.loadTenants(tenantSlug.value || undefined);
}

const createDialogOpen = ref(false);
function openCreateDialog() {
  // Drawer の dialog を閉じてから開き、作成ダイアログを入れ子にしない
  emit('close');
  createDialogOpen.value = true;
}

// ---- アイコンの右クリック・長押しで出すテナントのメニュー ----
const menu = ref<{ tenant: Tenant; x: number; y: number } | null>(null);
const tenantMenu = useTenantMenu(() => menu.value?.tenant ?? null);

function openMenu(tenant: Tenant, position: { x: number; y: number }) {
  menu.value = { tenant, ...position };
}

function onMenuSelect(value: string) {
  if (value !== 'copy-id') emit('close');
  tenantMenu.onSelect(value);
}

/**
 * レールの表示（ui-vue 0.15.0 の IconRail を包む TenantRail）は公開を待っている。
 * それまでは配線だけを置き、検査からも呼べるように出しておく。
 */
defineExpose({ selectTenant, retry, openCreateDialog, openMenu });
</script>

<template>
  <div class="flex h-full w-16 shrink-0 flex-col" data-slot="app-tenant-rail">
    <!-- TenantRail（ui-vue 0.15.0 の公開後に置く） -->
    <ContextMenu
      :open="menu !== null"
      :x="menu?.x ?? 0"
      :y="menu?.y ?? 0"
      :label="menu ? `${menu.tenant.name} のメニュー` : 'テナントのメニュー'"
      :items="tenantMenu.items.value"
      :on-select="onMenuSelect"
      :on-close="() => (menu = null)"
    />
    <Teleport to="body">
      <CreateTenantDialog v-model:open="createDialogOpen" />
    </Teleport>
  </div>
</template>
