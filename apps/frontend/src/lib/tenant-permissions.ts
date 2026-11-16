import type { components } from '@/generated/api';

type Tenant = components['schemas']['TenantListItemResponse'];
type Project = components['schemas']['ProjectResponse'];

/** `GET /v1/tenants` の行のうち、見ている本人の関わり方。未取得なら null / undefined */
type TenantView = Pick<Tenant, 'membership' | 'member_role'> | null | undefined;

/**
 * 権限の無い操作を出さないための判定。backend の認可と同じ式にそろえている。
 * テナントが未取得の間はすべて false（操作を出さない側へ倒す）。
 */

/** テナントオーナーか（backend の `is_tenant_owner` と同じ） */
export function isTenantOwner(tenant: TenantView): boolean {
  return tenant?.membership === 'Owner';
}

/** オーナーまたはテナント Admin か（backend の `require_tenant_admin` と同じ）。共有プロジェクトの作成に使う */
export function isTenantAdmin(tenant: TenantView): boolean {
  return isTenantOwner(tenant) || tenant?.member_role === 'Admin';
}

/**
 * プロジェクト本体（名前・説明・アイコン）の更新と削除ができるか
 * （backend の `require_project_manager` と同じ）。
 * 共有プロジェクトはオーナーとテナント Admin、個人プロジェクトはオーナーのみ。
 */
export function canManageProject(tenant: TenantView, project: Pick<Project, 'is_personal'>) {
  return project.is_personal ? isTenantOwner(tenant) : isTenantAdmin(tenant);
}
