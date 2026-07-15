import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { ref, nextTick } from 'vue';
import { mount, flushPromises, DOMWrapper } from '@vue/test-utils';
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

type MountOptions = {
  navigateOnSuccess?: boolean;
  open?: boolean;
};

function mountDialog(queryClient: QueryClient, options: MountOptions = {}) {
  return mount(CreateTaskDialog, {
    props: {
      open: options.open ?? true,
      tenantId: 'tenant-uuid',
      tenantDisplayId: 'tenant',
      projectId: 'project-uuid',
      projectKey: 'PROJ',
      statuses,
      navigateOnSuccess: options.navigateOnSuccess ?? false,
    },
    global: {
      plugins: [[VueQueryPlugin, { queryClient }]],
    },
    attachTo: document.body,
  });
}

function getTitleInput() {
  const input = document.body.querySelector('input[name="title"]');
  if (!input) throw new Error('title input not found');
  return input as HTMLInputElement;
}

function getForm() {
  const form = document.body.querySelector('form');
  if (!form) throw new Error('form not found');
  return form;
}

function getDialog() {
  const dialog = document.body.querySelector('[role="dialog"]');
  if (!dialog) throw new Error('dialog not found');
  return dialog as HTMLElement;
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
    await nextTick();

    const titleInput = new DOMWrapper(getTitleInput());
    await titleInput.setValue('New task');
    const form = new DOMWrapper(getForm());

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

describe('CreateTaskDialog a11y and cache invalidation', () => {
  let queryClient: QueryClient;
  let invalidateSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    isPending.value = false;
    mutateAsync.mockReset();
    mutateAsync.mockResolvedValue(createdTask);
    queryClient = new QueryClient({
      defaultOptions: {
        queries: { retry: false },
        mutations: { retry: false },
      },
    });
    invalidateSpy = vi
      .spyOn(queryClient, 'invalidateQueries')
      .mockResolvedValue(undefined as never);
  });

  afterEach(() => {
    vi.restoreAllMocks();
    document.body.innerHTML = '';
  });

  it('closes on Escape from a focused field', async () => {
    const wrapper = mountDialog(queryClient);
    await nextTick();
    const titleInput = getTitleInput();
    titleInput.focus();

    document.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }),
    );
    await nextTick();

    expect(wrapper.emitted('update:open')?.at(-1)).toEqual([false]);
    wrapper.unmount();
  });

  it('keeps Tab focus within the dialog', async () => {
    const wrapper = mountDialog(queryClient);
    await nextTick();
    const dialog = getDialog();
    const focusables = dialog.querySelectorAll<HTMLElement>(
      'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled])',
    );
    expect(focusables.length).toBeGreaterThan(1);

    focusables[0]?.focus();
    for (let i = 0; i < focusables.length + 2; i += 1) {
      document.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true }),
      );
      await nextTick();
      expect(dialog.contains(document.activeElement)).toBe(true);
    }

    wrapper.unmount();
  });

  it('invalidates the task list when staying on the page', async () => {
    const wrapper = mountDialog(queryClient, { navigateOnSuccess: false });
    await nextTick();
    const titleInput = new DOMWrapper(getTitleInput());
    await titleInput.setValue('New task');
    await new DOMWrapper(getForm()).trigger('submit');
    await flushPromises();

    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: ['get', '/v1/tenants/{tenant_id}/projects/{project_id}/tasks'],
    });
    wrapper.unmount();
  });

  it('skips invalidate when navigating away on success', async () => {
    const assign = vi.fn();
    vi.stubGlobal('location', { ...window.location, assign });

    const wrapper = mountDialog(queryClient, { navigateOnSuccess: true });
    await nextTick();
    const titleInput = new DOMWrapper(getTitleInput());
    await titleInput.setValue('New task');
    await new DOMWrapper(getForm()).trigger('submit');
    await flushPromises();

    expect(invalidateSpy).not.toHaveBeenCalled();
    expect(assign).toHaveBeenCalledWith('/tenant/projects/proj/tasks/1');
    wrapper.unmount();
    vi.unstubAllGlobals();
  });
});
