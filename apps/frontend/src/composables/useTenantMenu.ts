import { toast } from '@koyori-app/ui-vue';
import { usePageContext } from 'vike-vue/usePageContext';
import { navigate } from 'vike/client/router';
import { computed, toValue, type ComputedRef, type MaybeRefOrGetter } from 'vue';

import { useMeQuery } from '@/lib/api-vue-query';
import { useAuthStore } from '@/stores/auth';
import { useTenantStore, type Tenant } from '@/stores/tenant';

export interface TenantMenuItem {
  value: 'settings' | 'members' | 'copy-id';
  label: string;
}

/**
 * テナントのメニュー（サイドバーの見出しの「…」と、レールのアイコンの右クリック・長押し）。
 * 権限の出し分けをここに集め、どちらの入口でも同じ項目にする。
 */
export function useTenantMenu(tenant: MaybeRefOrGetter<Tenant | null>): {
  items: ComputedRef<TenantMenuItem[]>;
  onSelect: (value: string) => void;
} {
  const pageContext = usePageContext();
  const authStore = useAuthStore();
  const tenantStore = useTenantStore();
  const meQuery = useMeQuery();

  /** `/me` が返るまでは永続化された利用者で補う。初回描画で項目が欠けないようにする。 */
  const currentUserId = computed(() => meQuery.data.value?.id ?? authStore.user?.id);

  const items = computed<TenantMenuItem[]>(() => {
    const target = toValue(tenant);
    if (!target) return [];
    const result: TenantMenuItem[] = [];
    if (currentUserId.value && currentUserId.value === target.owner_id) {
      result.push({ value: 'settings', label: 'テナント設定' });
    }
    // メンバー一覧は project にだけ招かれた Guest には 403 が返るので、導線を出さない
    if (target.membership !== 'Guest') result.push({ value: 'members', label: 'メンバー' });
    result.push({ value: 'copy-id', label: '表示 ID をコピー' });
    return result;
  });

  function go(target: Tenant, path: string) {
    const href = `/${target.display_id}${path}`;
    if (target.display_id === pageContext.routeParams.tenant) {
      void navigate(href);
      return;
    }
    // テナントに紐づく状態を残さないため、別テナントへはフルページ遷移にする
    tenantStore.selectTenant(target);
    window.location.assign(href);
  }

  async function copyDisplayId(target: Tenant) {
    try {
      if (!navigator.clipboard) throw new Error('clipboard is unavailable');
      await navigator.clipboard.writeText(target.display_id);
      toast.success('コピーしました');
    } catch {
      toast.error('コピーできませんでした');
    }
  }

  function onSelect(value: string) {
    const target = toValue(tenant);
    if (!target) return;
    if (value === 'settings') go(target, '/settings');
    else if (value === 'members') go(target, '/settings/members');
    else if (value === 'copy-id') void copyDisplayId(target);
  }

  return { items, onSelect };
}
