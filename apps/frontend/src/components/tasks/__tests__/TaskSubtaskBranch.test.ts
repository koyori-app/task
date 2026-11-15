import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';
import { nextTick, toValue } from 'vue';
import type { UseTaskSubtasksParams } from '@/composables/useTaskSubtasks';

// 取得結果の変化に枝の watch が追従することも確かめるため、リアクティブな値で持つ
const subtasksState = await vi.hoisted(async () => {
  const { ref } = await import('vue');
  return {
    loading: ref(false),
    pending: ref(false),
    error: ref(false),
    subtasks: ref([] as Array<{ id: string; title: string; status_id: string }>),
    createPending: { value: false },
    createError: { value: null as string | null },
    createSubtask: vi.fn(async () => true),
    refetch: vi.fn(),
    params: null as UseTaskSubtasksParams | null,
  };
});

vi.mock('@/composables/useTaskSubtasks', () => ({
  useTaskSubtasks: (params: UseTaskSubtasksParams) => {
    subtasksState.params = params;
    return subtasksState;
  },
}));

import TaskSubtaskBranch from '@/components/tasks/TaskSubtaskBranch.vue';
import TaskSubtaskComposer from '@/components/tasks/TaskSubtaskComposer.vue';
import type { components } from '@/generated/api';

enableAutoUnmount(afterEach);

type TaskResponse = components['schemas']['TaskResponse'];

const parentTask = {
  id: 'parent-id',
  title: '親タスク',
  status_id: 'status-todo',
} as TaskResponse;

function mountBranch(
  pending: Record<string, 'status_id' | undefined>,
  implicit?: boolean,
  onEmpty?: (empty: boolean) => void,
) {
  return mount(TaskSubtaskBranch, {
    props: {
      projectKey: 'TEST',
      parentTask,
      tenantId: 'tenant-id',
      projectId: 'project-id',
      statuses: [],
      projectLabels: [],
      members: [],
      pending,
      errors: {},
      onComment: vi.fn(async () => true),
      implicit,
      onEmpty,
    },
    global: { stubs: { TaskGroupedRow: true } },
  });
}

describe('TaskSubtaskBranch', () => {
  beforeEach(() => {
    subtasksState.subtasks.value = [];
    subtasksState.loading.value = false;
    subtasksState.pending.value = false;
    subtasksState.error.value = false;
    subtasksState.createSubtask.mockClear();
  });

  it('表示中の条件を取得処理へ渡し、展開中の条件変更にも追従する', async () => {
    const wrapper = mountBranch({});
    expect(toValue(subtasksState.params!.filters)).toEqual({ is_archived: false });
    await wrapper.setProps({ filters: { label_id: 'X', status_id: 'todo' } });
    expect(toValue(subtasksState.params!.filters)).toEqual({
      is_archived: false,
      label_id: 'X',
      status_id: 'todo',
    });
    await wrapper.setProps({ filters: { label_id: 'Y', status_id: 'done', is_archived: true } });
    expect(toValue(subtasksState.params!.filters)).toEqual({
      is_archived: true,
      label_id: 'Y',
      status_id: 'done',
    });
  });

  it('親のステータス更新中は追加UIを無効化し、作成ハンドラも拒否する', async () => {
    const wrapper = mountBranch({});
    await wrapper.setProps({ pending: { [parentTask.id]: 'status_id' } });
    const composer = wrapper.findComponent(TaskSubtaskComposer);
    const onCreate = composer.props('onCreate') as (title: string) => Promise<boolean>;

    expect(composer.get('input').attributes('disabled')).toBeDefined();
    expect(await onCreate('競合する子')).toBe(false);
    expect(subtasksState.createSubtask).not.toHaveBeenCalled();
  });

  it('既存の子がある場合も親のステータス更新中は追加ボタンを無効化する', () => {
    subtasksState.subtasks.value = [
      { id: 'child-id', title: '子タスク', status_id: 'status-todo' },
    ];
    const wrapper = mountBranch({ [parentTask.id]: 'status_id' });
    const addButton = wrapper
      .findAll('button')
      .find((button) => button.text() === 'サブタスクを追加');

    expect(addButton).toBeDefined();
    expect(addButton!.attributes('disabled')).toBeDefined();
  });

  it('ルートへ繰り上がった子には空の作成欄を出さず、送信も拒否する', async () => {
    const wrapper = mountBranch({});
    const onCreate = wrapper.getComponent(TaskSubtaskComposer).props('onCreate');
    await wrapper.setProps({ parentTask: { ...parentTask, parent_task_id: 'hidden-parent-id' } });
    expect(wrapper.findComponent(TaskSubtaskComposer).exists()).toBe(false);
    expect(toValue(subtasksState.params!.parentTaskId)).toBe('hidden-parent-id');
    expect(await onCreate('作れない孫')).toBe(false);
    expect(subtasksState.createSubtask).not.toHaveBeenCalled();
  });

  it('既存の子を持つサブタスクでも追加ボタンを出さない', async () => {
    subtasksState.subtasks.value = [
      { id: 'grandchild-id', title: '既存の孫', status_id: 'status-todo' },
    ];
    const wrapper = mountBranch({});
    await wrapper.setProps({ parentTask: { ...parentTask, parent_task_id: 'hidden-parent-id' } });
    expect(
      wrapper.findAll('button').some((button) => button.text().includes('サブタスクを追加')),
    ).toBe(false);
  });

  it('implicit 無しでも結果が無い間は作成欄を出さず読み込み中を出す', () => {
    subtasksState.pending.value = true;
    const wrapper = mountBranch({});

    expect(wrapper.findComponent(TaskSubtaskComposer).exists()).toBe(false);
    expect(wrapper.text()).toContain('サブタスクを読み込み中…');
  });

  describe('環境設定で開いた枝（implicit）', () => {
    it('子が 0 件なら作成欄も追加ボタンも出さず、枝そのものを描かない', () => {
      const wrapper = mountBranch({}, true);

      expect(wrapper.find('[data-subtask-branch]').exists()).toBe(false);
      expect(wrapper.findComponent(TaskSubtaskComposer).exists()).toBe(false);
      expect(wrapper.text()).not.toContain('サブタスクを追加');
    });

    it('子が 1 件以上なら行と「サブタスクを追加」を描く', () => {
      subtasksState.subtasks.value = [
        { id: 'child-id', title: '子タスク', status_id: 'status-todo' },
      ];
      const wrapper = mountBranch({}, true);

      expect(wrapper.find('[data-subtask-branch]').exists()).toBe(true);
      expect(wrapper.findAllComponents({ name: 'TaskGroupedRow' })).toHaveLength(1);
      expect(wrapper.text()).toContain('サブタスクを追加');
    });

    it('implicit 無しで子が 0 件なら従来どおり作成欄を出す', () => {
      const wrapper = mountBranch({});

      expect(wrapper.find('[data-subtask-branch]').exists()).toBe(true);
      expect(wrapper.findComponent(TaskSubtaskComposer).exists()).toBe(true);
    });

    it('読み込み中は表示を出さない（implicit 無しでは出す）', () => {
      subtasksState.loading.value = true;

      expect(mountBranch({}, true).text()).not.toContain('サブタスクを読み込み中…');
      expect(mountBranch({}).text()).toContain('サブタスクを読み込み中…');
    });

    it('失敗は隠さず、再試行を出す', () => {
      subtasksState.error.value = true;
      const wrapper = mountBranch({}, true);

      expect(wrapper.text()).toContain('サブタスクを読み込めませんでした');
      expect(wrapper.findAll('button').some((b) => b.text() === '再試行')).toBe(true);
    });

    it('取得が成功して 0 件なら empty: true、子がいれば false を返す', () => {
      const none = mountBranch({}, true);
      expect(none.emitted('empty')).toEqual([[true]]);

      subtasksState.subtasks.value = [
        { id: 'child-id', title: '子タスク', status_id: 'status-todo' },
      ];
      const some = mountBranch({}, true);
      expect(some.emitted('empty')).toEqual([[false]]);
    });

    it('読み込み中は empty: true にならない', () => {
      subtasksState.loading.value = true;

      expect(mountBranch({}, true).emitted('empty')).toEqual([[false]]);
    });

    it('取得に失敗したら空とは扱わず（empty: true にならず）、失敗を出す', async () => {
      subtasksState.error.value = true;
      const wrapper = mountBranch({}, true);

      expect(wrapper.emitted('empty')).toEqual([[false]]);
      expect(wrapper.text()).toContain('サブタスクを読み込めませんでした');
    });

    it('クエリが無効で結果が無い間（pending）は空とは扱わず、結果が 0 件で届いて初めて true にする', async () => {
      subtasksState.pending.value = true;
      const wrapper = mountBranch({}, true);
      expect(wrapper.emitted('empty')).toEqual([[false]]);
      // 結果が無い間は、暗黙の枝でも作成欄を出さない
      expect(wrapper.findComponent(TaskSubtaskComposer).exists()).toBe(false);

      subtasksState.pending.value = false;
      await nextTick();
      expect(wrapper.emitted('empty')).toEqual([[false], [true]]);
    });

    it('結果が届いたあとの変化（子が増えた・減った）にも empty が追従する', async () => {
      const wrapper = mountBranch({}, true);
      subtasksState.subtasks.value = [
        { id: 'child-id', title: '子タスク', status_id: 'status-todo' },
      ];
      await nextTick();
      subtasksState.subtasks.value = [];
      await nextTick();

      expect(wrapper.emitted('empty')).toEqual([[true], [false], [true]]);
    });

    it('枝が消えるときは empty: false を返し、親の空の記録を残さない', () => {
      const onEmpty = vi.fn();
      const wrapper = mountBranch({}, true, onEmpty);
      expect(onEmpty.mock.calls).toEqual([[true]]);

      wrapper.unmount();
      expect(onEmpty.mock.calls).toEqual([[true], [false]]);
    });

    it('取得の再実行条件は implicit の枝だけに渡す（フォーカス復帰で全行を取り直さない）', () => {
      mountBranch({}, true);
      expect(toValue(subtasksState.params!.queryOptions)).toEqual({
        staleTime: 30_000,
        refetchOnWindowFocus: false,
      });

      mountBranch({});
      expect(toValue(subtasksState.params!.queryOptions)).toBeUndefined();
    });
  });
});
