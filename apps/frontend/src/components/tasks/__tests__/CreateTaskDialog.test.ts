import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { ref, nextTick } from 'vue';
import { mount, flushPromises } from '@vue/test-utils';
import { VueQueryPlugin, QueryClient } from '@tanstack/vue-query';

const isPending = ref(false);
const mutateAsync = vi.fn();

vi.mock('@/composables/useHydrated', () => ({
  useHydrated: () => ref(true),
}));

vi.mock('@/lib/task-display', () => ({
  taskDetailHref: vi.fn(() => '/tenant/projects/proj/tasks/1'),
}));

vi.mock('@/lib/api-vue-query', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/api-vue-query')>();
  return {
    ...actual,
    apiClient: {
      ...actual.apiClient,
      useMutation: vi.fn(() => ({
        isPending,
        mutateAsync,
      })),
    },
  };
});

import CreateTaskDialog from '../CreateTaskDialog.vue';

const statuses = [
  {
    id: 'status-1',
    name: 'Todo',
    color: '#2563eb',
    is_default: true,
    is_done_state: false,
    created_at: '2026-01-01T00:00:00.000Z',
    position: 0,
    project_id: 'project-uuid',
  },
];

const createdTask = {
  id: 'task-1',
  seq_id: 1,
  title: 'New task',
  status_id: 'status-1',
  priority: 'Medium',
};

function mountDialog(queryClient: QueryClient) {
  return mount(CreateTaskDialog, {
    props: {
      open: true,
      tenantId: 'tenant-uuid',
      tenantDisplayId: 'tenant',
      projectId: 'project-uuid',
      projectKey: 'PROJ',
      statuses,
      navigateOnSuccess: false,
    },
    global: {
      plugins: [[VueQueryPlugin, { queryClient }]],
    },
    attachTo: document.body,
  });
}

describe('CreateTaskDialog double-submit guard', () => {
  let queryClient: QueryClient;
  let resolveMutation: ((value: typeof createdTask) => void) | undefined;

  beforeEach(() => {
    isPending.value = false;
    mutateAsync.mockReset();
    resolveMutation = undefined;
    mutateAsync.mockImplementation(
      () =>
        new Promise<typeof createdTask>((resolve) => {
          isPending.value = true;
          resolveMutation = resolve;
        }),
    );
    queryClient = new QueryClient({
      defaultOptions: {
        queries: { retry: false },
        mutations: { retry: false },
      },
    });
    vi.spyOn(queryClient, 'invalidateQueries').mockResolvedValue(undefined as never);
  });

  afterEach(() => {
    vi.restoreAllMocks();
    document.body.innerHTML = '';
  });

  it('does not fire a second mutation while create is pending (submit + Enter)', async () => {
    const wrapper = mountDialog(queryClient);

    await wrapper.get('input[name="title"]').setValue('New task');
    const form = wrapper.get('form');

    await form.trigger('submit');
    await nextTick();

    expect(mutateAsync).toHaveBeenCalledTimes(1);
    expect(isPending.value).toBe(true);

    await form.trigger('submit');

    const enter = new KeyboardEvent('keydown', {
      key: 'Enter',
      bubbles: true,
      cancelable: true,
    });
    form.element.dispatchEvent(enter);
    await form.trigger('submit');
    await nextTick();

    expect(mutateAsync).toHaveBeenCalledTimes(1);

    resolveMutation?.(createdTask);
    await flushPromises();

    wrapper.unmount();
  });
});
