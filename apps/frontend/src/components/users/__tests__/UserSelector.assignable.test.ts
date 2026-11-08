import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { defineComponent, h } from 'vue';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { pickerOption, pickerTrigger } from '@/components/__tests__/picker';
import UserSelector from '@/components/users/UserSelector.vue';
import type { components } from '@/generated/api';
import type { ProjectUuid, TenantUuid } from '@/lib/api-ids';
import { useAssignableUsersQuery } from '@/lib/api-vue-query';

enableAutoUnmount(afterEach);
afterEach(() => vi.unstubAllGlobals());

const TENANT = '11111111-1111-4111-8111-111111111111' as TenantUuid;
const PROJECT = '22222222-2222-4222-8222-222222222222' as ProjectUuid;

/** 担当者の候補の口が返す JSON（生成した型で形を縛る）。 */
const assignableUsers: components['schemas']['AssignableUserResponse'][] = [
  {
    id: 'u-alice',
    username: 'alice',
    account_kind: 'human',
    avatar_url: null,
    department: null,
    team: null,
  },
  {
    id: 'u-bot',
    username: 'release-bot',
    account_kind: 'bot',
    avatar_url: null,
    department: null,
    team: null,
  },
];

function stubAssignableUsers(body: unknown) {
  vi.stubGlobal('fetch', async (request: Request) => {
    const pathname = new URL(request.url, 'http://localhost').pathname;
    if (pathname.endsWith(`/projects/${PROJECT}/assignable-users`)) {
      return new Response(JSON.stringify(body), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      });
    }
    return new Response(JSON.stringify({ message: 'not found' }), { status: 404 });
  });
}

/** 画面と同じく、担当者の候補の query の結果をそのまま UserSelector へ渡す。 */
const Harness = defineComponent({
  setup() {
    const query = useAssignableUsersQuery(TENANT, PROJECT);
    return () =>
      h(UserSelector, { label: '担当者', users: query.data.value ?? [], modelValue: [] });
  },
});

async function render() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = mount(Harness, {
    global: { plugins: [[VueQueryPlugin, { queryClient }]] },
    attachTo: document.body,
  });
  await flushPromises();
  return wrapper;
}

describe('担当者の候補の応答から BOT の補足を出す', () => {
  it('応答の account_kind が bot の候補にだけ「BOT アカウント」を添える', async () => {
    stubAssignableUsers(assignableUsers);
    const wrapper = await render();
    const trigger = pickerTrigger(wrapper.element, '担当者');

    expect(pickerOption(trigger, 'release-bot').textContent).toContain('BOT アカウント');
    expect(pickerOption(trigger, 'alice').textContent).not.toContain('BOT');
  });
});
