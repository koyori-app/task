import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import type { components } from '@/generated/api';
import { apiClient } from '@/lib/api';

export type Tenant = components['schemas']['TenantResponse'];

export const useTenantStore = defineStore(
  'tenant',
  () => {
    const tenants = ref<Tenant[]>([]);
    const selectedTenantId = ref<string | null>(null);
    const isLoading = ref(false);
    const error = ref<string | null>(null);

    const selectedTenant = computed(
      () => tenants.value.find((tenant) => tenant.id === selectedTenantId.value) ?? null,
    );

    function selectTenant(tenant: Tenant) {
      selectedTenantId.value = tenant.id;
    }

    async function loadTenants(routeTenant?: string) {
      isLoading.value = true;
      error.value = null;
      try {
        const response = await apiClient.GET('/v1/tenants');
        if (response.error) throw response.error;
        const payload: unknown = response.data;
        tenants.value = Array.isArray(payload) ? (payload as Tenant[]) : [];

        const routeMatch = tenants.value.find(
          (tenant) => tenant.display_id === routeTenant || tenant.id === routeTenant,
        );
        const persistedMatch = tenants.value.find((tenant) => tenant.id === selectedTenantId.value);
        const nextTenant = routeMatch ?? persistedMatch ?? tenants.value[0];
        selectedTenantId.value = nextTenant?.id ?? null;
      } catch {
        tenants.value = [];
        selectedTenantId.value = null;
        error.value = 'テナント一覧を取得できませんでした';
      } finally {
        isLoading.value = false;
      }
    }

    return {
      tenants,
      selectedTenantId,
      selectedTenant,
      isLoading,
      error,
      selectTenant,
      loadTenants,
    };
  },
  { persist: { pick: ['selectedTenantId'] } },
);
