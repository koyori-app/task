import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { createPinia } from 'pinia';
import { defineComponent } from 'vue';
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const { membersData, meData, updateMutateAsync, deleteMutateAsync } = vi.hoisted(() => ({
  membersData: { value: [] as unknown[] },
  meData: { value: null as unknown },
  updateMutateAsync: vi.fn(),
  deleteMutateAsync: vi.fn(),
}));

vi.mock('@/lib/api-vue-query', () => ({
  apiClient: {
    useQuery: () => ({
      data: membersData,
      isPending: { value: false },
      isError: { value: false },
    }),
    useMutation: (method: string) => ({
      mutateAsync: method === 'delete' ? deleteMutateAsync : updateMutateAsync,
      isPending: { value: false },
    }),
  },
  useMeQuery: () => ({ data: meData }),
}));

import TenantSettingsMembers from '../TenantSettingsMembers.vue';
import type { components } from '@/generated/api';

type TenantMemberResponse = components['schemas']['TenantMemberResponse'];

const TENANT_ID = '11111111-1111-4111-8111-111111111111';
const OWNER_ID = '22222222-2222-4222-8222-222222222222';
const ADMIN_ID = '33333333-3333-4333-8333-333333333333';
const MEMBER_ID = '44444444-4444-4444-8444-444444444444';

function member(userId: string, username: string, role: 'Admin' | 'Member'): TenantMemberResponse {
  return {
    id: `row-${userId}`,
    tenant_id: TENANT_ID,
    user_id: userId,
    role,
    user: { id: userId, username, avatar_url: null },
  };
}

const PassThrough = defineComponent({ template: '<div><slot /></div>' });
const ButtonStub = defineComponent({
  inheritAttrs: false,
  template: '<button v-bind="$attrs"><slot /></button>',
});

let wrapper: VueWrapper;

/** 操作するのは Admin 本人。オーナーは別人なので、自分の行にも除名ボタンの条件が効く。 */
function mountMembers() {
  membersData.value = [
    member(OWNER_ID, 'owner', 'Admin'),
    member(ADMIN_ID, 'admin', 'Admin'),
    member(MEMBER_ID, 'member', 'Member'),
  ];
  meData.value = { id: ADMIN_ID, username: 'admin', avatar_url: null };
  wrapper = mount(TenantSettingsMembers, {
    props: { tenant: { id: TENANT_ID, name: 'Alpha', owner_id: OWNER_ID } },
    global: {
      plugins: [
        createPinia(),
        [
          VueQueryPlugin,
          { queryClient: new QueryClient({ defaultOptions: { queries: { retry: false } } }) },
        ],
      ],
      stubs: {
        Button: ButtonStub,
        Select: PassThrough,
        SelectContent: PassThrough,
        SelectItem: PassThrough,
        SelectTrigger: PassThrough,
      },
    },
  });
  return wrapper;
}

describe('TenantSettingsMembers', () => {
  beforeEach(() => {
    updateMutateAsync.mockReset().mockResolvedValue(undefined);
    deleteMutateAsync.mockReset().mockResolvedValue(undefined);
  });

  afterEach(() => {
    wrapper?.unmount();
    vi.restoreAllMocks();
  });

  it('Admin 本人の除名は塞ぐが、他のメンバーは外せる', async () => {
    const wrapper = mountMembers();

    const self = wrapper.get('button[aria-label="adminを外す"]');
    expect(self.attributes('disabled')).toBeDefined();
    await self.trigger('click');
    await flushPromises();
    expect(deleteMutateAsync).not.toHaveBeenCalled();

    const other = wrapper.get('button[aria-label="memberを外す"]');
    expect(other.attributes('disabled')).toBeUndefined();
    await other.trigger('click');
    await flushPromises();
    expect(deleteMutateAsync).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_ID, user_id: MEMBER_ID } },
    });
  });

  it('処理中の連打で同じ要求を二重に送らない', async () => {
    let finish!: () => void;
    deleteMutateAsync.mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          finish = () => resolve();
        }),
    );
    const wrapper = mountMembers();

    const other = wrapper.get('button[aria-label="memberを外す"]');
    await other.trigger('click');
    await other.trigger('click');
    expect(deleteMutateAsync).toHaveBeenCalledTimes(1);

    finish();
    await flushPromises();
    expect(deleteMutateAsync).toHaveBeenCalledTimes(1);
  });
});
