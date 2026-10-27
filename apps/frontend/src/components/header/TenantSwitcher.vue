<script setup lang="ts">
import { PhBuildings } from '@phosphor-icons/vue';
import { useQuery } from '@tanstack/vue-query';
import { navigate } from 'vike/client/router';
import { computed, ref } from 'vue';

import CreateTenantDialog from './CreateTenantDialog.vue';
import type { Tenant } from '@/stores/tenant';
import { Dropdown, type DropdownItem, type DropdownSection } from '@koyori-app/ui-vue';
import { apiClient, useMeQuery } from '@/lib/api-vue-query';
import { useAuthStore } from '@/stores/auth';

const MEMBERS_PATH = '/v1/tenants/{tenant_id}/members' as const;

const props = defineProps<{
  tenants: Tenant[];
  selectedTenantId: string | null;
  loading?: boolean;
  error?: string | null;
}>();

const emit = defineEmits<{
  select: [tenant: Tenant];
  retry: [];
}>();

const createDialogOpen = ref(false);
const activeTenant = computed(
  () => props.tenants.find((tenant) => tenant.id === props.selectedTenantId) ?? null,
);
const tenantNotFound = computed(
  () => !props.loading && !props.error && props.tenants.length > 0 && !activeTenant.value,
);

/** 引き金に出す名前。読み込み中・見つからないときはその事情を出す。 */
const triggerLabel = computed(() => {
  if (props.loading) return 'テナントを読み込み中…';
  if (tenantNotFound.value) return '指定されたテナントが見つかりません';
  return activeTenant.value?.name ?? '所属テナントなし';
});

/**
 * メンバー一覧はテナントに入れる人だけが読める。project にだけ招かれた客分
 * （membership が Guest）には 403 が返るので、導線もメンバー数も出さない。
 */
const canSeeMembers = computed(
  () => !!activeTenant.value && activeTenant.value.membership !== 'Guest',
);

/**
 * 頭のメンバー数。テナントが決まるまでは呼ばない。
 *
 * 参照デザインはここにプラン名も並べるが、テナントにプランの概念が無いので数だけ出す。
 */
const membersQuery = useQuery(
  computed(() => ({
    ...apiClient.queryOptions('get', MEMBERS_PATH, {
      params: { path: { tenant_id: activeTenant.value?.id ?? '' } },
    }),
    enabled: canSeeMembers.value,
    staleTime: 60_000,
    retry: false,
  })),
);

const authStore = useAuthStore();
const meQuery = useMeQuery();
/**
 * `/me` が返るまでは永続化された利用者で補う。AppHeader の歯車と同じ式にしないと、
 * 初回描画で歯車は出るのにこの項目だけ出ない、という食い違いが出る。
 */
const currentUserId = computed(() => meQuery.data.value?.id ?? authStore.user?.id);
const canManageGeneral = computed(
  () =>
    !!activeTenant.value &&
    !!currentUserId.value &&
    currentUserId.value === activeTenant.value.owner_id,
);

const memberSummary = computed(() => {
  const count = membersQuery.data.value?.length;
  return count === undefined ? '' : `${count} 人のメンバー`;
});

/** テナント設定への行き先。テナントが決まっていないと組み立てられない。 */
const settingsHref = computed(() =>
  activeTenant.value ? `/${activeTenant.value.display_id}/settings` : '',
);
const membersHref = computed(() =>
  activeTenant.value ? `/${activeTenant.value.display_id}/settings/members` : '',
);

const TENANT_PREFIX = 'tenant:';

const sections = computed<DropdownSection[]>(() => {
  const result: DropdownSection[] = [];
  // 今どのテナントに居るか。切り替えの前に、開いた人の現在地を出す
  if (activeTenant.value) {
    const items: DropdownItem[] = [];
    if (canManageGeneral.value) items.push({ value: 'settings', label: 'テナント設定' });
    if (canSeeMembers.value) items.push({ value: 'members', label: 'メンバー' });
    result.push({
      heading: memberSummary.value
        ? `${activeTenant.value.name}（${memberSummary.value}）`
        : activeTenant.value.name,
      items,
    });
  }

  const switchItems: DropdownItem[] = [];
  if (props.error) switchItems.push({ value: 'retry', label: `${props.error}（再試行）` });
  else if (!props.loading && props.tenants.length === 0) {
    switchItems.push({ value: 'empty', label: '所属テナントがありません', disabled: true });
  }
  for (const tenant of props.tenants) {
    switchItems.push({
      value: `${TENANT_PREFIX}${tenant.id}`,
      label: tenant.name,
      kind: 'radio',
      checked: tenant.id === props.selectedTenantId,
    });
  }
  result.push({ heading: 'テナントを切り替え', items: switchItems });
  result.push({ items: [{ value: 'create', label: 'テナントを作成' }] });
  return result;
});

function onSelect(value: string) {
  if (value === 'settings') void navigate(settingsHref.value);
  else if (value === 'members') void navigate(membersHref.value);
  else if (value === 'retry') emit('retry');
  else if (value === 'create') createDialogOpen.value = true;
  else if (value.startsWith(TENANT_PREFIX)) {
    const tenant = props.tenants.find((t) => `${TENANT_PREFIX}${t.id}` === value);
    if (tenant) emit('select', tenant);
  }
}
</script>

<template>
  <Dropdown
    variant="tertiary"
    :label="triggerLabel"
    :disabled="loading"
    :items="sections"
    :on-select="onSelect"
  >
    <template #trigger>
      <span class="flex max-w-60 items-center gap-2 text-left">
        <span
          class="flex aspect-square size-6 shrink-0 items-center justify-center overflow-hidden rounded-md border bg-secondary"
        >
          <img
            v-if="activeTenant?.icon_url"
            :src="activeTenant.icon_url"
            alt=""
            class="size-full object-cover"
          />
          <span v-else-if="activeTenant?.icon_emoji" class="text-sm leading-none">{{
            activeTenant.icon_emoji
          }}</span>
          <PhBuildings v-else class="size-3.5 text-muted-foreground" />
        </span>
        <span class="truncate font-medium">{{ triggerLabel }}</span>
      </span>
    </template>
  </Dropdown>
  <CreateTenantDialog v-model:open="createDialogOpen" />
</template>
