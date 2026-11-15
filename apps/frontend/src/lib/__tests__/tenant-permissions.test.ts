import { describe, expect, it } from 'vitest';
import { canManageProject, isTenantAdmin, isTenantOwner } from '../tenant-permissions';

type View = Parameters<typeof isTenantOwner>[0];

const owner: View = { membership: 'Owner', member_role: null };
const admin: View = { membership: 'Member', member_role: 'Admin' };
const member: View = { membership: 'Member', member_role: 'Member' };
const viewer: View = { membership: 'Member', member_role: 'Viewer' };
const guest: View = { membership: 'Guest', member_role: null };

describe('tenant-permissions', () => {
  it.each([
    ['Owner', owner, true],
    ['Admin', admin, false],
    ['Member', member, false],
    ['Viewer', viewer, false],
    ['Guest', guest, false],
    ['null', null, false],
    ['undefined', undefined, false],
  ] as const)('isTenantOwner: %s', (_name, tenant, expected) => {
    expect(isTenantOwner(tenant)).toBe(expected);
  });

  it.each([
    ['Owner', owner, true],
    ['Admin', admin, true],
    ['Member', member, false],
    ['Viewer', viewer, false],
    ['Guest', guest, false],
    ['null', null, false],
    ['undefined', undefined, false],
  ] as const)('isTenantAdmin: %s', (_name, tenant, expected) => {
    expect(isTenantAdmin(tenant)).toBe(expected);
  });

  it.each([
    ['Owner', owner, true, true],
    ['Admin', admin, true, false],
    ['Member', member, false, false],
    ['Viewer', viewer, false, false],
    ['Guest', guest, false, false],
    ['null', null, false, false],
  ] as const)(
    'canManageProject: %s（共有 / 個人）',
    (_name, tenant, expectedShared, expectedPersonal) => {
      expect(canManageProject(tenant, { is_personal: false })).toBe(expectedShared);
      expect(canManageProject(tenant, { is_personal: true })).toBe(expectedPersonal);
    },
  );
});
