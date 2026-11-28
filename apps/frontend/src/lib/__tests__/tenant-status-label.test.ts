import { describe, expect, it } from 'vitest';
import type { Tenant } from '@/stores/tenant';
import { tenantStatusLabel } from '../tenant-status-label';

const alpha: Tenant = {
  id: 'tenant-1',
  display_id: 'alpha',
  name: 'Alpha',
  description: '',
  icon_url: '',
  owner_id: 'owner-1',
  require_2fa: false,
  membership: 'Owner',
};

describe('tenantStatusLabel', () => {
  it('says it is loading while the list is loading', () => {
    expect(
      tenantStatusLabel({ loading: true, error: null, tenants: [alpha], selected: alpha }),
    ).toBe('テナントを読み込み中…');
  });

  it('says the tenant is not found when the URL tenant is not in the list', () => {
    expect(
      tenantStatusLabel({ loading: false, error: null, tenants: [alpha], selected: null }),
    ).toBe('指定されたテナントが見つかりません');
  });

  it('shows the name of the selected tenant', () => {
    expect(
      tenantStatusLabel({ loading: false, error: null, tenants: [alpha], selected: alpha }),
    ).toBe('Alpha');
  });

  it('says there is no tenant when the list is empty', () => {
    expect(tenantStatusLabel({ loading: false, error: null, tenants: [], selected: null })).toBe(
      '所属テナントなし',
    );
  });

  it('shows the error when the list failed to load', () => {
    expect(
      tenantStatusLabel({
        loading: false,
        error: 'テナント一覧を取得できませんでした',
        tenants: [],
        selected: null,
      }),
    ).toBe('テナント一覧を取得できませんでした');
  });
});
