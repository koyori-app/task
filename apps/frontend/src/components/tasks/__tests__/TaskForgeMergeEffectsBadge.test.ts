import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';

import TaskForgeMergeEffectsBadge from '../TaskForgeMergeEffectsBadge.vue';

enableAutoUnmount(afterEach);
afterEach(() => {
  vi.unstubAllGlobals();
});

const pullRequest = {
  id: 'pr-1',
  // タスクのプロジェクトとは別の、PR 行の持ち主
  project_id: 'pr-project',
  host: 'github',
  host_url: 'https://github.com',
  repo_owner: 'acme',
  repo_name: 'backend',
  number: 87,
  title: 'feat: OAuth 対応 TASK-1',
  author_handle: 'yupix',
  state: 'merged',
  html_url: 'https://github.com/acme/backend/pull/87',
  merged_at: '2026-10-01T00:00:00Z',
  merge_effects_processed_at: null as string | null,
  commits_complete: false,
  sources: ['body'],
  closes: true,
  unresolved: null,
};

function mountBadge(overrides: Partial<typeof pullRequest> = {}, status = 200) {
  const requests: Request[] = [];
  vi.stubGlobal('fetch', async (request: Request) => {
    requests.push(request);
    return status === 200
      ? new Response(null, { status })
      : Response.json({ message: 'conflict' }, { status });
  });
  const queryClient = new QueryClient();
  const invalidate = vi.spyOn(queryClient, 'invalidateQueries');
  const wrapper = mount(TaskForgeMergeEffectsBadge, {
    props: { tenantId: 'tenant-1', pullRequest: { ...pullRequest, ...overrides } },
    global: { plugins: [[VueQueryPlugin, { queryClient }]] },
    attachTo: document.body,
  });
  return { wrapper, requests, invalidate };
}

function button(name: string): HTMLButtonElement {
  const found = [...document.body.querySelectorAll('button')].find(
    (b) => b.textContent?.trim() === name,
  );
  if (!found) throw new Error(`button not found: ${name}`);
  return found;
}

describe('TaskForgeMergeEffectsBadge', () => {
  it.each([
    ['処理済み', { merge_effects_processed_at: '2026-10-01T00:10:00Z' }],
    ['マージされていない', { state: 'open', merged_at: null as unknown as string }],
  ])('%s PR には出さない', (_, overrides) => {
    const { wrapper } = mountBadge(overrides);
    expect(wrapper.find('[data-merge-effects-badge]').exists()).toBe(false);
  });

  it('未適用の PR に印を出し、確認を経て PR 行のプロジェクトの口へ送る', async () => {
    const { wrapper, requests, invalidate } = mountBadge();
    const badge = wrapper.get('[data-merge-effects-badge]');
    expect(badge.text()).toBe('マージ済み（自動完了は未適用）');

    await badge.trigger('click');
    await flushPromises();
    expect(document.body.textContent).toContain('#87 の自動完了は未適用です');
    expect(document.body.textContent).toContain('再配送');

    button('確認済みにする…').click();
    await flushPromises();
    expect(document.body.querySelector('[data-merge-effects-confirm]')).not.toBeNull();
    expect(requests).toHaveLength(0);

    button('確認済みにする').click();
    await flushPromises();
    expect(requests).toHaveLength(1);
    expect(requests[0]!.method).toBe('POST');
    expect(new URL(requests[0]!.url).pathname).toBe(
      '/api/v1/tenants/tenant-1/projects/pr-project/forge/pull-requests/pr-1/merge-effects/acknowledge',
    );
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: ['get', '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/forge'],
    });
    expect(document.body.textContent).not.toContain('#87 の自動完了は未適用です');
  });

  it('失敗したらダイアログに残してエラーを出す', async () => {
    const { wrapper } = mountBadge({}, 409);
    await wrapper.get('[data-merge-effects-badge]').trigger('click');
    await flushPromises();
    button('確認済みにする…').click();
    await flushPromises();
    button('確認済みにする').click();
    await flushPromises();
    expect(document.body.querySelector('[role="alert"]')?.textContent).toBe(
      '確認済みにできませんでした',
    );
  });
});
