import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DOMWrapper, enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { OpenApiVueQueryError } from '@koyori-app/openapi-vue-query';

const { mutateAsync } = vi.hoisted(() => ({ mutateAsync: vi.fn() }));

vi.mock('@/lib/api-vue-query', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/api-vue-query')>();
  return {
    ...actual,
    apiClient: {
      ...actual.apiClient,
      useMutation: vi.fn(() => ({ mutateAsync, isPending: { value: false } })),
    },
  };
});

import TaskForgePullRequestLinkDialog from '../TaskForgePullRequestLinkDialog.vue';

enableAutoUnmount(afterEach);

function mountDialog(repoFullName: string | null = 'acme/backend') {
  return mount(TaskForgePullRequestLinkDialog, {
    props: { tenantId: 'tenant-1', projectId: 'project-1', taskId: 'TASK-1', repoFullName },
    global: { plugins: [[VueQueryPlugin, { queryClient: new QueryClient() }]] },
    attachTo: document.body,
  });
}

async function submitWith(number: string) {
  await new DOMWrapper(document.body.querySelector('#forge-pr-number')!).setValue(number);
  await new DOMWrapper(document.body.querySelector('form')!).trigger('submit');
  await flushPromises();
}

describe('TaskForgePullRequestLinkDialog', () => {
  beforeEach(() => {
    mutateAsync.mockReset();
    document.body.innerHTML = '';
  });

  it('連携先を表示し、番号と closes を送って閉じる', async () => {
    mutateAsync.mockResolvedValue({});
    const wrapper = mountDialog();
    await flushPromises();
    expect(document.body.textContent).toContain('acme/backend の PR');

    document.body.querySelector<HTMLButtonElement>('button[role="checkbox"]')!.click();
    await flushPromises();
    await submitWith(' 87 ');

    expect(mutateAsync).toHaveBeenCalledWith({
      params: { path: { tenant_id: 'tenant-1', project_id: 'project-1', id: 'TASK-1' } },
      body: { number: 87, closes: true },
    });
    expect(wrapper.emitted('close')).toHaveLength(1);
  });

  it('番号でなければ送らない', async () => {
    mountDialog(null);
    await flushPromises();
    expect(document.body.textContent).toContain('プロジェクトに連携したリポジトリの PR');

    for (const value of ['', '0', 'abc', '#87']) {
      await submitWith(value);
    }
    expect(mutateAsync).not.toHaveBeenCalled();
    expect(document.body.textContent).toContain('PR 番号を数字で入力してください');
  });

  it('連携が無い（409）ならその旨を出して閉じない', async () => {
    mutateAsync.mockRejectedValue(
      new OpenApiVueQueryError(
        { message: 'github-integration-missing' },
        new Response(null, { status: 409 }),
      ),
    );
    const wrapper = mountDialog();
    await flushPromises();
    await submitWith('87');

    expect(document.body.textContent).toContain('このプロジェクトは GitHub と連携していません');
    expect(wrapper.emitted('close')).toBeUndefined();
  });
});
