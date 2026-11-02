import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { enableAutoUnmount, flushPromises, mount, type VueWrapper } from '@vue/test-utils';
import { nextTick, ref } from 'vue';

const subtasksState = vi.hoisted(() => ({
  loading: { value: false },
  error: { value: false },
  subtasks: { value: [] as Array<{ id: string; title: string; status_id: string }> },
  parentTask: { value: null as { id: string; seq_id: number; title: string } | null },
  createPending: { value: false },
  createError: { value: null as string | null },
  createSubtask: vi.fn(async () => true),
  refetch: vi.fn(),
}));

vi.mock('@/composables/useTaskSubtasks', () => ({
  useTaskSubtasks: () => subtasksState,
}));

import TaskParentPicker from '@/components/tasks/TaskParentPicker.vue';
import TaskSubtaskPanel from '@/components/tasks/TaskSubtaskPanel.vue';
import TaskSubtaskComposer from '@/components/tasks/TaskSubtaskComposer.vue';

enableAutoUnmount(afterEach);

function mountPanel(
  statusUpdating = false,
  overrides: { parentTaskId?: string | null; parentUpdating?: boolean } = {},
) {
  return mount(TaskSubtaskPanel, {
    props: {
      tenantId: 'tenant-id',
      projectId: 'project-id',
      taskId: 'TASK-1',
      taskUuid: 'task-id',
      parentTaskId: overrides.parentTaskId ?? null,
      statusId: 'status-todo',
      statusUpdating,
      parentUpdating: overrides.parentUpdating ?? false,
      parentError: null,
      statuses: [],
      projectKey: 'TASK',
    },
    global: { plugins: [[VueQueryPlugin, { queryClient: new QueryClient() }]] },
  });
}

function buttonByText(wrapper: VueWrapper, text: string) {
  return wrapper.findAll('button').find((button) => button.text().includes(text));
}

function jsonResponse(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

const searchInput = 'input[aria-label="親にするタスクを検索"]';
const draftInput = 'input[aria-label="サブタスク名"]';

describe('TaskSubtaskPanel', () => {
  beforeEach(() => {
    subtasksState.subtasks.value = [];
    subtasksState.parentTask.value = null;
    subtasksState.createPending = { value: false };
    subtasksState.createSubtask.mockClear();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('ステータス更新中は追加ボタンからComposerを開けない', async () => {
    const wrapper = mountPanel(true);
    const addButton = buttonByText(wrapper, 'サブタスクを追加')!;

    expect(addButton.attributes('disabled')).toBeDefined();
    await addButton.trigger('click');
    expect(wrapper.findComponent(TaskSubtaskComposer).exists()).toBe(false);
  });

  it('Composerを開いた後にステータス更新が始まっても作成を拒否する', async () => {
    const wrapper = mountPanel();
    await buttonByText(wrapper, 'サブタスクを追加')!.trigger('click');
    await wrapper.setProps({ statusUpdating: true });

    const composer = wrapper.findComponent(TaskSubtaskComposer);
    const onCreate = composer.props('onCreate') as (title: string) => Promise<boolean>;
    expect(composer.get('input').attributes('disabled')).toBeDefined();
    expect(await onCreate('競合する子')).toBe(false);
    expect(subtasksState.createSubtask).not.toHaveBeenCalled();
  });

  it('親が取得できない子でも追加UIを出さず、既存の子への導線は残す', async () => {
    subtasksState.subtasks.value = [
      { id: 'grandchild-id', title: '既存の孫', status_id: 'status-todo' },
    ];
    const wrapper = mountPanel();
    await wrapper.setProps({ parentTaskId: 'deleted-parent-id' });
    expect(wrapper.findComponent(TaskSubtaskComposer).exists()).toBe(false);
    expect(buttonByText(wrapper, 'サブタスクを追加')).toBeUndefined();
    await wrapper.get('li button').trigger('click');
    expect(wrapper.emitted('open')?.[0]).toEqual([subtasksState.subtasks.value[0]]);
  });

  it('Composerを開いた後に子タスクへ変わったら入力欄を閉じ、保持された送信も拒否する', async () => {
    const wrapper = mountPanel();
    await buttonByText(wrapper, 'サブタスクを追加')!.trigger('click');
    const onCreate = wrapper.getComponent(TaskSubtaskComposer).props('onCreate');
    await wrapper.setProps({ parentTaskId: 'parent-id' });
    expect(wrapper.findComponent(TaskSubtaskComposer).exists()).toBe(false);
    expect(await onCreate('作れない孫')).toBe(false);
    expect(subtasksState.createSubtask).not.toHaveBeenCalled();
  });

  it('親の変更が拒否されたら、Composerの下書きを残して入力できる状態へ戻す', async () => {
    const wrapper = mountPanel();
    await buttonByText(wrapper, 'サブタスクを追加')!.trigger('click');
    await wrapper.get(draftInput).setValue('書きかけの子');

    // 楽観更新で親付きに見える間も閉じず、作成だけ止める
    await wrapper.setProps({ parentTaskId: 'descendant-id', parentUpdating: true });
    const composer = wrapper.getComponent(TaskSubtaskComposer);
    expect(composer.get(draftInput).attributes('disabled')).toBeDefined();
    const onCreate = composer.props('onCreate') as (title: string) => Promise<boolean>;
    expect(await onCreate('確定前の子')).toBe(false);
    expect(subtasksState.createSubtask).not.toHaveBeenCalled();

    await wrapper.setProps({
      parentTaskId: null,
      parentUpdating: false,
      parentError: '自分自身や子孫のタスクは親にできません',
    });
    const input = wrapper.get<HTMLInputElement>(draftInput);
    expect(input.element.value).toBe('書きかけの子');
    expect(input.attributes('disabled')).toBeUndefined();
  });

  it('親の変更が確定したらComposerを閉じ、後で親を変更・解除しても開き直さない', async () => {
    const wrapper = mountPanel();
    await buttonByText(wrapper, 'サブタスクを追加')!.trigger('click');
    await wrapper.setProps({ parentTaskId: 'parent-id', parentUpdating: true });
    await wrapper.setProps({ parentUpdating: false });
    expect(wrapper.findComponent(TaskSubtaskComposer).exists()).toBe(false);

    await wrapper.setProps({ parentTaskId: 'other-parent-id', parentUpdating: true });
    expect(wrapper.findComponent(TaskSubtaskComposer).exists()).toBe(false);

    await wrapper.setProps({ parentTaskId: null, parentUpdating: false });
    expect(wrapper.findComponent(TaskSubtaskComposer).exists()).toBe(false);
    expect(buttonByText(wrapper, 'サブタスクを追加')).toBeDefined();
  });

  describe('親タスクの操作', () => {
    it('親が無いタスクには「親を設定」だけを出す', () => {
      const wrapper = mountPanel();
      expect(buttonByText(wrapper, '親を設定')).toBeDefined();
      expect(buttonByText(wrapper, '親を変更')).toBeUndefined();
      expect(buttonByText(wrapper, '親を外す')).toBeUndefined();
    });

    it('親があるタスクは親への導線に加えて「親を変更」「親を外す」を出す', async () => {
      subtasksState.parentTask.value = { id: 'parent-id', seq_id: 5, title: '親のタイトル' };
      const wrapper = mountPanel(false, { parentTaskId: 'parent-id' });
      expect(wrapper.text()).toContain('TASK-5');
      expect(buttonByText(wrapper, '親を変更')).toBeDefined();
      expect(buttonByText(wrapper, '親を設定')).toBeUndefined();

      await buttonByText(wrapper, '親のタイトル')!.trigger('click');
      expect(wrapper.emitted('open')?.[0]).toEqual([subtasksState.parentTask.value]);
    });

    it('親を取得できなくても変更と解除はできる', () => {
      const wrapper = mountPanel(false, { parentTaskId: 'deleted-parent-id' });
      expect(buttonByText(wrapper, '親を変更')).toBeDefined();
      expect(buttonByText(wrapper, '親を外す')).toBeDefined();
    });

    it('楽観更新中は古い relations の親を今の親として描かない', () => {
      subtasksState.parentTask.value = { id: 'old-parent', seq_id: 5, title: '元の親' };
      const wrapper = mountPanel(false, { parentTaskId: 'new-parent', parentUpdating: true });
      expect(wrapper.text()).not.toContain('元の親');
    });

    it('「親を外す」は null を emit する', async () => {
      const wrapper = mountPanel(false, { parentTaskId: 'parent-id' });
      await buttonByText(wrapper, '親を外す')!.trigger('click');
      expect(wrapper.emitted('change-parent')?.[0]).toEqual([null]);
    });

    it('親の変更中は親の操作もサブタスクの追加も止める', async () => {
      const wrapper = mountPanel(false, { parentUpdating: true });
      expect(buttonByText(wrapper, '親を設定')!.attributes('disabled')).toBeDefined();
      const addButton = buttonByText(wrapper, 'サブタスクを追加')!;
      expect(addButton.attributes('disabled')).toBeDefined();
      await addButton.trigger('click');
      expect(wrapper.findComponent(TaskSubtaskComposer).exists()).toBe(false);
    });

    it('親のエラーを表示する', async () => {
      const wrapper = mountPanel();
      await wrapper.setProps({ parentError: '自分自身や子孫のタスクは親にできません' });
      expect(wrapper.get('[role="alert"]').text()).toBe('自分自身や子孫のタスクは親にできません');
    });

    it('別のタスクへ移ったら候補の入力欄を閉じる', async () => {
      const wrapper = mountPanel();
      await buttonByText(wrapper, '親を設定')!.trigger('click');
      expect(wrapper.findComponent(TaskParentPicker).exists()).toBe(true);
      await wrapper.setProps({ taskUuid: 'other-task-id' });
      expect(wrapper.findComponent(TaskParentPicker).exists()).toBe(false);
    });

    it('Esc と閉じるボタンで候補の入力欄を閉じる', async () => {
      const wrapper = mountPanel();
      await buttonByText(wrapper, '親を設定')!.trigger('click');
      await wrapper.get(searchInput).trigger('keydown', { key: 'Escape' });
      expect(wrapper.findComponent(TaskParentPicker).exists()).toBe(false);

      await buttonByText(wrapper, '親を設定')!.trigger('click');
      await wrapper.get('button[aria-label="親タスクの選択をやめる"]').trigger('click');
      expect(wrapper.findComponent(TaskParentPicker).exists()).toBe(false);
    });
  });

  describe('親の候補', () => {
    function stubApi(handler: (url: string) => Response | undefined) {
      const fetchMock = vi.fn(async (request: Request) => {
        return handler(request.url) ?? jsonResponse({ message: 'not found' }, 404);
      });
      vi.stubGlobal('fetch', fetchMock);
      return fetchMock;
    }

    function searchHit(id: string, seq_id: number, title: string) {
      return { id, seq_id, title, highlight: title, score: 1 };
    }

    async function openAndType(wrapper: VueWrapper, text: string) {
      await buttonByText(wrapper, '親を')!.trigger('click');
      await wrapper.get(searchInput).setValue(text);
    }

    function candidateTexts(wrapper: VueWrapper) {
      return wrapper.findAll('[data-task-parent-picker] li').map((item) => item.text());
    }

    it('タイトルで検索し、選ぶと親の id を emit して閉じる', async () => {
      stubApi((url) =>
        url.includes('/tasks/search')
          ? jsonResponse({ tasks: [searchHit('hit-id', 7, '候補のタスク')], total: 1 })
          : undefined,
      );
      const wrapper = mountPanel();
      await openAndType(wrapper, '候補');

      await vi.waitFor(() => expect(candidateTexts(wrapper)).toHaveLength(1));
      expect(candidateTexts(wrapper)[0]).toContain('TASK-7');

      await wrapper.get('[data-task-parent-picker] li button').trigger('click');
      expect(wrapper.emitted('change-parent')?.[0]).toEqual(['hit-id']);
      expect(wrapper.findComponent(TaskParentPicker).exists()).toBe(false);
    });

    it('サブタスクの作成中は親の操作と候補の選択を止める', async () => {
      const createPending = ref(false);
      subtasksState.createPending = createPending;
      stubApi((url) =>
        url.includes('/tasks/search')
          ? jsonResponse({ tasks: [searchHit('hit-id', 7, '候補のタスク')], total: 1 })
          : undefined,
      );
      const wrapper = mountPanel();
      await openAndType(wrapper, '候補');
      await vi.waitFor(() => expect(candidateTexts(wrapper)).toHaveLength(1));

      createPending.value = true;
      await nextTick();

      expect(buttonByText(wrapper, '親を設定')!.attributes('disabled')).toBeDefined();
      const candidate = wrapper.get('[data-task-parent-picker] li button');
      expect(candidate.attributes('disabled')).toBeDefined();
      await candidate.trigger('click');
      expect(wrapper.emitted('change-parent')).toBeUndefined();

      await wrapper.setProps({ parentTaskId: 'parent-id' });
      expect(buttonByText(wrapper, '親を変更')!.attributes('disabled')).toBeDefined();
      expect(buttonByText(wrapper, '親を外す')!.attributes('disabled')).toBeDefined();
    });

    it('自分自身・今の親・直下の子を候補から除く', async () => {
      subtasksState.subtasks.value = [{ id: 'child-id', title: '子', status_id: 'status-todo' }];
      stubApi((url) =>
        url.includes('/tasks/search')
          ? jsonResponse({
              tasks: [
                searchHit('task-id', 1, '自分'),
                searchHit('parent-id', 2, '今の親'),
                searchHit('child-id', 3, '子'),
                searchHit('other-id', 4, '他のタスク'),
              ],
              total: 4,
            })
          : undefined,
      );
      const wrapper = mountPanel(false, { parentTaskId: 'parent-id' });
      await openAndType(wrapper, 'タスク');

      await vi.waitFor(() => expect(candidateTexts(wrapper)).toHaveLength(1));
      expect(candidateTexts(wrapper)[0]).toContain('他のタスク');
    });

    it.each(['task-9', 'TASK-9', '9'])(
      '検索 API はキーに当たらないので、%s は TASK-9 を直接引いて先頭に出す',
      async (typed) => {
        const fetchMock = stubApi((url) => {
          if (url.includes('/tasks/search')) {
            // キーのタスクが本文検索にも当たっても 2 回出さない
            return jsonResponse({
              tasks: [searchHit('nine-id', 9, 'キーで引くタスク'), searchHit('x-id', 10, '言及')],
              total: 2,
            });
          }
          if (url.endsWith('/tasks/TASK-9')) {
            return jsonResponse({ id: 'nine-id', seq_id: 9, title: 'キーで引くタスク' });
          }
          return undefined;
        });
        const wrapper = mountPanel();
        await openAndType(wrapper, typed);

        await vi.waitFor(() => expect(candidateTexts(wrapper)).toHaveLength(2));
        expect(candidateTexts(wrapper)[0]).toContain('TASK-9');
        expect(fetchMock.mock.calls.some(([req]) => req.url.endsWith('/tasks/TASK-9'))).toBe(true);
      },
    );

    it('キーではない語はタスク本体を引かない', async () => {
      const fetchMock = stubApi((url) =>
        url.includes('/tasks/search') ? jsonResponse({ tasks: [], total: 0 }) : undefined,
      );
      const wrapper = mountPanel();
      await openAndType(wrapper, 'OAuth');

      await vi.waitFor(() => expect(wrapper.text()).toContain('一致する候補がありません'));
      expect(fetchMock.mock.calls.every(([req]) => req.url.includes('/tasks/search'))).toBe(true);
    });

    it('検索に失敗したら候補なしと混ぜずにエラーを出す', async () => {
      stubApi((url) =>
        url.includes('/tasks/search') ? jsonResponse({ message: 'boom' }, 500) : undefined,
      );
      const wrapper = mountPanel();
      await openAndType(wrapper, '候補');

      await vi.waitFor(() =>
        expect(wrapper.get('[data-task-parent-picker] [role="alert"]').text()).toBe(
          '検索できませんでした',
        ),
      );
      expect(wrapper.text()).not.toContain('一致する候補がありません');
    });

    it('キーの取得に失敗したら、該当なしではなくエラーを出す', async () => {
      stubApi((url) => {
        if (url.includes('/tasks/search')) return jsonResponse({ tasks: [], total: 0 });
        return url.endsWith('/tasks/TASK-9') ? jsonResponse({ message: 'boom' }, 500) : undefined;
      });
      const wrapper = mountPanel();
      await openAndType(wrapper, 'TASK-9');

      await vi.waitFor(() =>
        expect(wrapper.get('[data-task-parent-picker] [role="alert"]').text()).toBe(
          '検索できませんでした',
        ),
      );
    });

    it('入力が検索に追いつくまで前の語の候補を出さない', async () => {
      stubApi((url) =>
        url.includes('/tasks/search')
          ? jsonResponse({ tasks: [searchHit('hit-id', 7, '前の語の候補')], total: 1 })
          : undefined,
      );
      const wrapper = mountPanel();
      await openAndType(wrapper, '前');
      await vi.waitFor(() => expect(candidateTexts(wrapper)).toHaveLength(1));

      await wrapper.get(searchInput).setValue('次の語');
      await flushPromises();
      expect(candidateTexts(wrapper)).toHaveLength(0);
      expect(wrapper.text()).toContain('検索中…');
    });
  });
});
