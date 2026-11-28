import type { Tenant } from '@/stores/tenant';

export interface TenantStatusInput {
  loading: boolean;
  error: string | null;
  tenants: readonly Tenant[];
  selected: Tenant | null;
}

/**
 * 今のテナントとして出す名前。読み込み中・取得の失敗・URL のテナントが一覧に無いときはその事情を出す。
 * サイドバーの見出しと、狭い画面の本文の上の帯で同じ文言にする。
 */
export function tenantStatusLabel({ loading, error, tenants, selected }: TenantStatusInput) {
  if (loading) return 'テナントを読み込み中…';
  if (error && !selected) return error;
  if (!error && tenants.length > 0 && !selected) return '指定されたテナントが見つかりません';
  return selected?.name ?? '所属テナントなし';
}
