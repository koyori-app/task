import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { createPinia } from 'pinia';
import { defineComponent } from 'vue';
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const {
  membersData,
  invitationsData,
  meData,
  updateMutateAsync,
  deleteMutateAsync,
  inviteMutateAsync,
  resendMutateAsync,
  revokeMutateAsync,
} = vi.hoisted(() => ({
  membersData: { value: [] as unknown[] },
  invitationsData: { value: [] as unknown[] },
  meData: { value: null as unknown },
  updateMutateAsync: vi.fn(),
  deleteMutateAsync: vi.fn(),
  inviteMutateAsync: vi.fn(),
  resendMutateAsync: vi.fn(),
  revokeMutateAsync: vi.fn(),
}));

vi.mock('@/lib/api-vue-query', () => ({
  apiClient: {
    useQuery: (_method: string, path: string) => ({
      data: path.endsWith('/invitations') ? invitationsData : membersData,
      isPending: { value: false },
      isError: { value: false },
    }),
    useMutation: (method: string, path: string) => ({
      mutateAsync: path.includes('/invitations')
        ? ({ post: path.endsWith('/resend') ? resendMutateAsync : inviteMutateAsync }[method] ??
          revokeMutateAsync)
        : method === 'delete'
          ? deleteMutateAsync
          : updateMutateAsync,
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
function mountMembers(asUserId = ADMIN_ID) {
  membersData.value = [
    member(OWNER_ID, 'owner', 'Admin'),
    member(ADMIN_ID, 'admin', 'Admin'),
    member(MEMBER_ID, 'member', 'Member'),
  ];
  meData.value = { id: asUserId, username: 'me', avatar_url: null };
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
    invitationsData.value = [];
    updateMutateAsync.mockReset().mockResolvedValue(undefined);
    deleteMutateAsync.mockReset().mockResolvedValue(undefined);
    inviteMutateAsync.mockReset().mockResolvedValue(undefined);
    resendMutateAsync.mockReset().mockResolvedValue(undefined);
    revokeMutateAsync.mockReset().mockResolvedValue(undefined);
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

  it('メールアドレスとロールで招待を送り、入力を空にする', async () => {
    const wrapper = mountMembers();

    const submit = wrapper.get('form button[type="submit"]');
    expect(submit.attributes('disabled'), '形の崩れたアドレスでは送れない').toBeDefined();

    await wrapper.get('input[type="email"]').setValue(' new@example.com ');
    expect(submit.attributes('disabled')).toBeUndefined();
    await wrapper.get('form').trigger('submit');
    await flushPromises();

    expect(inviteMutateAsync).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_ID } },
      body: { email: 'new@example.com', role: 'Member' },
    });
    expect(wrapper.get('[role="status"]').text()).toContain('new@example.com に招待を送りました');
    expect((wrapper.get('input[type="email"]').element as HTMLInputElement).value).toBe('');
  });

  it('既にメンバーのアドレスは理由を出し、入力を残す', async () => {
    inviteMutateAsync.mockRejectedValue({ response: { status: 409 } });
    const wrapper = mountMembers();

    await wrapper.get('input[type="email"]').setValue('member@example.com');
    await wrapper.get('form').trigger('submit');
    await flushPromises();

    expect(wrapper.get('[role="alert"]').text()).toContain('既にメンバーです');
    expect((wrapper.get('input[type="email"]').element as HTMLInputElement).value).toBe(
      'member@example.com',
    );
  });

  it('保留中の招待を期限つきで並べ、再送と取り消しを対応する口へ送る', async () => {
    invitationsData.value = [
      {
        id: 'inv-live',
        tenant_id: TENANT_ID,
        email: 'live@example.com',
        role: 'Member',
        expires_at: '2999-01-01T00:00:00Z',
        created_at: '2026-01-01T00:00:00Z',
      },
      {
        id: 'inv-expired',
        tenant_id: TENANT_ID,
        email: 'expired@example.com',
        role: 'Viewer',
        expires_at: '2000-01-01T00:00:00Z',
        created_at: '2000-01-01T00:00:00Z',
      },
    ];
    const wrapper = mountMembers();

    const rows = wrapper.findAll('li').filter((li) => li.text().includes('@example.com'));
    expect(rows).toHaveLength(2);
    expect(rows[0]!.text()).not.toContain('期限切れ');
    expect(rows[1]!.text()).toContain('期限切れ');

    await wrapper.get('button[aria-label="expired@example.comへ招待を送り直す"]').trigger('click');
    await flushPromises();
    expect(resendMutateAsync).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_ID, invitation_id: 'inv-expired' } },
    });

    await wrapper.get('button[aria-label="live@example.comへの招待を取り消す"]').trigger('click');
    await flushPromises();
    expect(revokeMutateAsync).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_ID, invitation_id: 'inv-live' } },
    });
  });

  it('管理者でなければ招待の欄を出さない', () => {
    const wrapper = mountMembers(MEMBER_ID);
    expect(wrapper.find('input[type="email"]').exists()).toBe(false);
  });
});
