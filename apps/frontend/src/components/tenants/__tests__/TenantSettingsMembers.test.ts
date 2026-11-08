import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { createPinia } from 'pinia';
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
import {
  accessibleText,
  pickerOption,
  pickerOptions,
  pickerTrigger,
} from '@/components/__tests__/picker';
import type { components } from '@/generated/api';

type TenantMemberResponse = components['schemas']['TenantMemberResponse'];

const TENANT_ID = '11111111-1111-4111-8111-111111111111';
const OWNER_ID = '22222222-2222-4222-8222-222222222222';
const ADMIN_ID = '33333333-3333-4333-8333-333333333333';
const MEMBER_ID = '44444444-4444-4444-8444-444444444444';

function member(
  userId: string,
  username: string,
  role: 'Admin' | 'Member',
  accountKind = 'human',
): TenantMemberResponse {
  return {
    id: `row-${userId}`,
    tenant_id: TENANT_ID,
    user_id: userId,
    role,
    user: { id: userId, username, account_kind: accountKind, avatar_url: null },
    department: null,
    position: null,
    team: null,
  };
}

let wrapper: VueWrapper;

/** 操作するのは Admin 本人。オーナーは別人なので、自分の行にも除名ボタンの条件が効く。 */
function mountMembers(
  asUserId = ADMIN_ID,
  rows = [
    member(OWNER_ID, 'owner', 'Admin'),
    member(ADMIN_ID, 'admin', 'Admin'),
    member(MEMBER_ID, 'member', 'Member'),
  ],
) {
  membersData.value = rows;
  meData.value = { id: asUserId, username: 'me', avatar_url: null };
  wrapper = mount(TenantSettingsMembers, {
    props: { tenant: { id: TENANT_ID, display_id: 'alpha', name: 'Alpha', owner_id: OWNER_ID } },
    global: {
      plugins: [
        createPinia(),
        [
          VueQueryPlugin,
          { queryClient: new QueryClient({ defaultOptions: { queries: { retry: false } } }) },
        ],
      ],
      // BOT の欄は己の試験（TenantBotsSection.test.ts）で縛る。ここではメンバーの欄だけを見る
      stubs: { TenantBotsSection: true },
    },
  });
  return wrapper;
}

function actionButton(label: string) {
  return wrapper
    .get('dialog[open]')
    .findAll('button')
    .find((b) => b.text().trim() === label)!;
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

  it('BOT の行の役の選び手には Admin を出さず、人の行には出す', () => {
    const BOT_ID = '55555555-5555-4555-8555-555555555555';
    const wrapper = mountMembers(ADMIN_ID, [
      member(OWNER_ID, 'owner', 'Admin'),
      member(ADMIN_ID, 'admin', 'Admin'),
      member(MEMBER_ID, 'member', 'Member'),
      member(BOT_ID, 'release-bot', 'Member', 'bot'),
    ]);
    const roleNames = (label: string) =>
      pickerOptions(pickerTrigger(wrapper.element, label)).map((option) => accessibleText(option));

    expect(roleNames('release-botのロール').some((name) => name.startsWith('Admin'))).toBe(false);
    expect(roleNames('release-botのロール').some((name) => name.startsWith('Member'))).toBe(true);
    expect(roleNames('memberのロール').some((name) => name.startsWith('Admin'))).toBe(true);
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
    expect(deleteMutateAsync).not.toHaveBeenCalled();
    await actionButton('外す').trigger('click');
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
    await actionButton('外す').trigger('click');
    await actionButton('外しています…').trigger('click');
    expect(wrapper.find('dialog[open]').exists()).toBe(true);
    expect(deleteMutateAsync).toHaveBeenCalledTimes(1);

    finish();
    await flushPromises();
    expect(wrapper.find('dialog[open]').exists()).toBe(false);
    expect(deleteMutateAsync).toHaveBeenCalledTimes(1);
  });

  it('除外は確認を経てから DELETE を送り、キャンセルすれば送らない', async () => {
    mountMembers();
    const remove = wrapper.get('button[aria-label="memberを外す"]');
    await remove.trigger('click');
    expect(deleteMutateAsync).not.toHaveBeenCalled();
    expect(wrapper.get('dialog[open]').text()).toContain('「member」をこのテナントから外します');
    await actionButton('キャンセル').trigger('click');
    expect(deleteMutateAsync).not.toHaveBeenCalled();
    expect(wrapper.find('dialog[open]').exists()).toBe(false);
    await remove.trigger('click');
    await actionButton('外す').trigger('click');
    await flushPromises();
    expect(deleteMutateAsync).toHaveBeenCalledTimes(1);
    expect(wrapper.find('dialog[open]').exists()).toBe(false);
  });

  it('除外に失敗したら理由をダイアログに出し、閉じない', async () => {
    deleteMutateAsync.mockRejectedValue(new Error('failed'));
    mountMembers();
    await wrapper.get('button[aria-label="memberを外す"]').trigger('click');
    await actionButton('外す').trigger('click');
    await flushPromises();
    expect(wrapper.get('dialog[open] [role="alert"]').text()).toContain(
      'メンバーを外せませんでした',
    );
  });

  it('自分を Admin から降格するときは失う操作を説明して確認する', async () => {
    mountMembers();
    const select = async () => {
      pickerOption(pickerTrigger(wrapper.element, 'adminのロール'), 'Member').click();
      await flushPromises();
    };
    await select();
    expect(updateMutateAsync).not.toHaveBeenCalled();
    expect(wrapper.get('dialog[open]').text()).toContain('招待・ロール変更・除外ができなくなり');
    await actionButton('キャンセル').trigger('click');
    expect(updateMutateAsync).not.toHaveBeenCalled();
    expect(pickerTrigger(wrapper.element, 'adminのロール').getAttribute('aria-label')).toBe(
      'adminのロール: Admin',
    );
    await select();
    await actionButton('変更する').trigger('click');
    await flushPromises();
    expect(updateMutateAsync).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_ID, user_id: ADMIN_ID } },
      body: { role: 'Member' },
    });
    expect(wrapper.find('dialog[open]').exists()).toBe(false);
  });

  it('他人のロール変更は確認なしで送る', async () => {
    mountMembers();
    pickerOption(pickerTrigger(wrapper.element, 'memberのロール'), 'Viewer').click();
    await flushPromises();
    expect(updateMutateAsync).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_ID, user_id: MEMBER_ID } },
      body: { role: 'Viewer' },
    });
    expect(wrapper.find('dialog[open]').exists()).toBe(false);
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
    // Picker も読み上げ用の role=status を持つので、知らせの段落に絞る
    expect(wrapper.get('p[role="status"]').text()).toContain('new@example.com に招待を送りました');
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

  it('所属をダイアログから保存すると、部署・役職・チームだけを送る（role は送らない）', async () => {
    const wrapper = mountMembers(ADMIN_ID, [
      member(OWNER_ID, 'owner', 'Admin'),
      member(ADMIN_ID, 'admin', 'Admin'),
      { ...member(MEMBER_ID, 'member', 'Member'), department: '開発部', team: '基盤' },
    ]);
    expect(wrapper.text()).toContain('開発部 · 基盤');

    await wrapper.get('button[aria-label="memberの所属を編集"]').trigger('click');
    expect(wrapper.get('dialog').attributes('open')).toBeDefined();
    expect((wrapper.get('#org-profile-department').element as HTMLInputElement).value).toBe(
      '開発部',
    );
    expect(wrapper.get('#org-profile-department').attributes('maxlength')).toBe('100');
    await wrapper.get('#org-profile-position').setValue('課長');
    await wrapper.get('#org-profile-team').setValue('');
    await wrapper.findAll('form').at(-1)!.trigger('submit');
    await flushPromises();

    expect(updateMutateAsync).toHaveBeenCalledTimes(1);
    expect(updateMutateAsync).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_ID, user_id: MEMBER_ID } },
      body: { department: '開発部', position: '課長', team: '' },
    });
    expect(wrapper.get('dialog').attributes('open'), '保存後は閉じる').toBeUndefined();
  });

  it('オーナー本人は自分の所属を保存でき、再読み込み後も表示される', async () => {
    const wrapper = mountMembers(OWNER_ID);
    await wrapper.get('button[aria-label="ownerの所属を編集"]').trigger('click');
    expect(wrapper.get('dialog').attributes('open')).toBeDefined();
    await wrapper.get('#org-profile-department').setValue('経営企画');
    await wrapper.get('#org-profile-position').setValue('代表');
    await wrapper.get('#org-profile-team').setValue('役員');
    await wrapper.findAll('form').at(-1)!.trigger('submit');
    await flushPromises();

    expect(updateMutateAsync).toHaveBeenCalledTimes(1);
    expect(updateMutateAsync).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_ID, user_id: OWNER_ID } },
      body: { department: '経営企画', position: '代表', team: '役員' },
    });
    expect(wrapper.get('dialog').attributes('open'), '保存後は閉じる').toBeUndefined();

    // 再読み込み: 保存した値が行に出て、編集欄にも戻る
    wrapper.unmount();
    const reloaded = mountMembers(OWNER_ID, [
      {
        ...member(OWNER_ID, 'owner', 'Admin'),
        department: '経営企画',
        position: '代表',
        team: '役員',
      },
    ]);
    expect(reloaded.text()).toContain('経営企画 · 代表 · 役員');
    await reloaded.get('button[aria-label="ownerの所属を編集"]').trigger('click');
    expect((reloaded.get('#org-profile-position').element as HTMLInputElement).value).toBe('代表');
  });

  it('オーナー本人でも自分のロール変更と除外の操作は出さない', () => {
    const wrapper = mountMembers(OWNER_ID);
    expect(wrapper.find('button[aria-label="ownerを外す"]').exists()).toBe(false);
    expect(wrapper.find('[aria-label="ownerのロール"]').exists()).toBe(false);
    // 対照: 他のメンバーの操作は残っている
    expect(wrapper.find('button[aria-label="memberを外す"]').exists()).toBe(true);
    expect(wrapper.find('button[aria-label="memberの所属を編集"]').exists()).toBe(true);
  });

  it('所属の編集ボタンは管理者にだけ出し、他人のオーナー行には出さない', () => {
    const asAdmin = mountMembers();
    expect(asAdmin.find('button[aria-label="memberの所属を編集"]').exists()).toBe(true);
    expect(asAdmin.find('button[aria-label="ownerの所属を編集"]').exists()).toBe(false);
    asAdmin.unmount();

    const asMember = mountMembers(MEMBER_ID);
    expect(asMember.find('button[aria-label="memberの所属を編集"]').exists()).toBe(false);
    expect(asMember.find('button[aria-label="adminの所属を編集"]').exists()).toBe(false);
  });
});
