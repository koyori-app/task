import { describe, expect, it } from 'vitest';

import { rolesForAccount } from '@/lib/bot-roles';

const ROLES = [{ value: 'Admin' }, { value: 'Member' }, { value: 'Viewer' }] as const;

describe('rolesForAccount', () => {
  it('BOT には Admin を出さず、Member と Viewer だけを出す', () => {
    expect(rolesForAccount(ROLES, 'bot').map((role) => role.value)).toEqual(['Member', 'Viewer']);
  });

  it('人と種別の無い利用者には全ての役を出す', () => {
    expect(rolesForAccount(ROLES, 'human').map((role) => role.value)).toEqual([
      'Admin',
      'Member',
      'Viewer',
    ]);
    expect(rolesForAccount(ROLES, undefined)).toHaveLength(3);
  });
});
