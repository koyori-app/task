import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { defineComponent, nextTick, type Component } from 'vue';

// 実物の TaskSubtaskBranch を使う試験のため、取得だけをリアクティブな値に差し替える
const subtasksState = await vi.hoisted(async () => {
  const { ref } = await import('vue');
  return {
    loading: ref(false),
    pending: ref(false),
    error: ref(false),
    subtasks: ref<unknown[]>([]),
    createPending: ref(false),
    createError: ref<string | null>(null),
    createSubtask: async () => true,
    refetch: () => {},
  };
});

vi.mock('@/composables/useTaskSubtasks', () => ({ useTaskSubtasks: () => subtasksState }));

import TaskGroupedList from '@/components/tasks/TaskGroupedList.vue';
import type { TaskGroup } from '@/components/tasks/task-grouped-columns';
import type { components } from '@/generated/api';
import type { CreateTaskInput } from '@/composables/useTaskRowMutations';

enableAutoUnmount(afterEach);

type StatusResponse = components['schemas']['ProjectStatusResponse'];
type LabelResponse = components['schemas']['LabelResponse'];
type TaskResponse = components['schemas']['TaskResponse'];

const status: StatusResponse = {
  id: 'status-todo',
  name: 'Todo',
  color: '#94a3b8',
  position: 0,
  is_default: true,
  is_done_state: false,
  is_default_done: false,
  project_id: 'project-1',
  created_at: '2026-06-01T00:00:00Z',
};

const bug: LabelResponse = {
  id: 'label-bug',
  name: 'bug',
  description: '',
  color: '#e11d48',
  icon_url: null,
  project_id: 'project-1',
};

const members = [
  { id: 'user-1', username: 'yupix', avatar_url: null },
  { id: 'user-2', username: 'sousuke', avatar_url: null },
];

const group: TaskGroup = {
  status,
  tasks: [],
  total: 0,
  isLoading: false,
  isError: false,
  hasMore: false,
  oldestFirst: true,
  retry: () => {},
  loadMore: () => {},
};

/** 順序の確認にだけ使う行。表示に要る欄だけ埋める。 */
function taskFixture(id: string, title: string): TaskResponse {
  return {
    id,
    project_id: 'project-1',
    seq_id: 1,
    title,
    description: null,
    status_id: status.id,
    priority: 'Medium',
    progress_pct: 0,
    soft_deadline: null,
    hard_deadline: null,
    is_archived: false,
    assignees: [],
    labels: [],
    created_at: '2026-06-01T00:00:00Z',
    updated_at: '2026-06-01T00:00:00Z',
  };
}

const doingStatus: StatusResponse = { ...status, id: 'status-doing', name: 'Doing', position: 1 };
const doingGroup: TaskGroup = { ...group, status: doingStatus };

/** 作成の受け口。成否を返す契約なので、既定は成功にする。 */
function mountList(
  onCreate = vi.fn(async (_input: CreateTaskInput) => true),
  groups: TaskGroup[] = [group],
  statuses: StatusResponse[] = [status],
  options: { expandSubtasks?: boolean; stubs?: Record<string, Component> } = {},
) {
  const wrapper = mount(TaskGroupedList, {
    global: options.stubs ? { stubs: options.stubs } : undefined,
    props: {
      expandSubtasks: options.expandSubtasks,
      groups,
      tenantId: 'tenant-1',
      projectId: 'project-1',
      projectKey: 'TASK',
      statuses,
      projectLabels: [bug],
      members,
      pending: {},
      errors: {},
      sorting: [],
      onComment: vi.fn(async () => true),
      onCreate,
      onMoveStatus: vi.fn(async () => undefined),
    },
    attachTo: document.body,
  });
  return { wrapper, onCreate };
}

/** グループは `section` 単位。同じ文言のボタンが並ぶので、対象の節の中から探す。 */
function sectionOf(wrapper: ReturnType<typeof mountList>['wrapper'], statusName: string) {
  const section = wrapper
    .findAll('section')
    .find((s) => s.find(`[aria-label="${statusName} を折りたたむ"]`).exists());
  expect(section).toBeDefined();
  return section!;
}

async function openAddRow(wrapper: ReturnType<typeof mountList>['wrapper'], statusName = 'Todo') {
  const addButton = sectionOf(wrapper, statusName)
    .findAll('button')
    .find((b) => b.text() === 'タスクを追加');
  expect(addButton).toBeDefined();
  await addButton!.trigger('click');
  await nextTick();
}

describe('TaskGroupedList のタスク追加', () => {
  it('タイトルだけでも作れる（未指定の項目は送らない）', async () => {
    const { wrapper, onCreate } = mountList();
    await openAddRow(wrapper);

    await wrapper.get('input[aria-label="Todo にタスクを追加"]').setValue('最小のタスク');
    const save = wrapper.findAll('button').find((b) => b.text().includes('保存'));
    await save!.trigger('click');

    expect(onCreate.mock.calls).toEqual([
      [
        {
          title: '最小のタスク',
          statusId: 'status-todo',
          assigneeIds: [],
          softDeadline: null,
          priority: null,
          labelIds: [],
        },
      ],
    ]);
  });

  it('その場で決めた担当者・期限・ラベルを一緒に送る', async () => {
    const { wrapper, onCreate } = mountList();
    await openAddRow(wrapper);

    await wrapper.get('input[aria-label="Todo にタスクを追加"]').setValue('設定つきタスク');
    // 期限はアイコンを押してから入力欄が出る（参照どおりアイコンだけ並べるため）
    await wrapper.get('button[aria-label="期限を設定"]').trigger('click');
    await nextTick();
    await wrapper.get('input[aria-label="期限"]').setValue('2026-09-15');

    // 担当者とラベルはメニュー越しなので、コンポーネントの入力口を直接叩く
    const picker = wrapper.findComponent({ name: 'TaskAssigneePicker' });
    picker.vm.$emit('toggle', 'user-2', true);
    await nextTick();

    const save = wrapper.findAll('button').find((b) => b.text().includes('保存'));
    await save!.trigger('click');

    expect(onCreate).toHaveBeenCalledTimes(1);
    expect(onCreate.mock.calls[0][0]).toMatchObject({
      title: '設定つきタスク',
      statusId: 'status-todo',
      assigneeIds: ['user-2'],
      // 日付だけの入力は詳細画面と同じヘルパーで ISO へ寄せる
      softDeadline: '2026-09-15T00:00:00.000Z',
    });
  });

  it('空のタイトルでは作らず、行を閉じる', async () => {
    const { wrapper, onCreate } = mountList();
    await openAddRow(wrapper);

    const save = wrapper.findAll('button').find((b) => b.text().includes('保存'));
    // 空のときは保存を押せない（誤って空タスクを作らない）
    expect(save!.attributes('disabled')).toBeDefined();

    const cancel = wrapper.findAll('button').find((b) => b.text() === 'キャンセル');
    await cancel!.trigger('click');
    await nextTick();

    expect(onCreate).not.toHaveBeenCalled();
    expect(wrapper.find('input[aria-label="Todo にタスクを追加"]').exists()).toBe(false);
  });

  it('作成後も入力欄は開いたまま、下書きは消える（続けて足せる）', async () => {
    const { wrapper } = mountList();
    await openAddRow(wrapper);

    const input = wrapper.get('input[aria-label="Todo にタスクを追加"]');
    await input.setValue('1件目');
    const save = wrapper.findAll('button').find((b) => b.text().includes('保存'));
    await save!.trigger('click');
    await nextTick();

    expect(
      wrapper.get<HTMLInputElement>('input[aria-label="Todo にタスクを追加"]').element.value,
    ).toBe('');
  });

  it('作成に失敗したら下書きを残す（消すと打ち直しになる）', async () => {
    const { wrapper } = mountList(vi.fn(async () => false));
    await openAddRow(wrapper);

    await wrapper.get('input[aria-label="Todo にタスクを追加"]').setValue('失敗するタスク');
    const save = wrapper.findAll('button').find((b) => b.text().includes('保存'));
    await save!.trigger('click');
    await nextTick();

    expect(
      wrapper.get<HTMLInputElement>('input[aria-label="Todo にタスクを追加"]').element.value,
    ).toBe('失敗するタスク');
  });

  // 下書きは全グループで 1 組を共有している。作成中も「キャンセル」と他グループの
  // 「タスクを追加」は押せるので、完了時に無条件でリセットすると別の下書きが消える
  it('作成中に別グループで書き始めたら、そちらの下書きを消さない', async () => {
    let resolveCreate: ((created: boolean) => void) | undefined;
    const onCreate = vi.fn(
      (_input: CreateTaskInput) =>
        new Promise<boolean>((resolve) => {
          resolveCreate = resolve;
        }),
    );
    const { wrapper } = mountList(onCreate, [group, doingGroup], [status, doingStatus]);

    await openAddRow(wrapper, 'Todo');
    await wrapper.get('input[aria-label="Todo にタスクを追加"]').setValue('Todo のタスク');
    const save = sectionOf(wrapper, 'Todo')
      .findAll('button')
      .find((b) => b.text().includes('保存'));
    await save!.trigger('click');

    // 保存中に別グループへ切り替えて書き始める
    await openAddRow(wrapper, 'Doing');
    await wrapper.get('input[aria-label="Doing にタスクを追加"]').setValue('Doing の書きかけ');

    resolveCreate!(true);
    await flushPromises();

    expect(
      wrapper.get<HTMLInputElement>('input[aria-label="Doing にタスクを追加"]').element.value,
    ).toBe('Doing の書きかけ');
  });

  // 同じグループを開き直した場合も、下書きはもうその作成のものではない
  it('作成中にキャンセルして同じグループで書き直したら、その下書きを消さない', async () => {
    let resolveCreate: ((created: boolean) => void) | undefined;
    const onCreate = vi.fn(
      (_input: CreateTaskInput) =>
        new Promise<boolean>((resolve) => {
          resolveCreate = resolve;
        }),
    );
    const { wrapper } = mountList(onCreate);

    await openAddRow(wrapper);
    await wrapper.get('input[aria-label="Todo にタスクを追加"]').setValue('1 件目');
    const save = wrapper.findAll('button').find((b) => b.text().includes('保存'));
    await save!.trigger('click');

    // 保存中にキャンセルし、開き直して別の下書きを書く
    const cancel = wrapper.findAll('button').find((b) => b.text().includes('キャンセル'));
    await cancel!.trigger('click');
    await openAddRow(wrapper);
    await wrapper.get('input[aria-label="Todo にタスクを追加"]').setValue('書き直した 1 件目');

    resolveCreate!(true);
    await flushPromises();

    expect(
      wrapper.get<HTMLInputElement>('input[aria-label="Todo にタスクを追加"]').element.value,
    ).toBe('書き直した 1 件目');
  });

  // 続きは古い側に積まれるので、ボタンが行の下にあると押した場所の周りが変わらない
  it('もっと見る をタスク行より上に出す', async () => {
    const { wrapper } = mountList();
    await wrapper.setProps({
      groups: [
        {
          ...group,
          tasks: [taskFixture('task-old', '古い 1 件目'), taskFixture('task-new', '新しい 1 件目')],
          total: 30,
          hasMore: true,
        },
      ],
    });
    await nextTick();

    const html = wrapper.html();
    expect(html).toContain('もっと見る（残り 28 件）');
    expect(html.indexOf('もっと見る')).toBeLessThan(html.indexOf('古い 1 件目'));
  });

  it('任意の並びでは もっと見る をタスク行より下に出す', async () => {
    const { wrapper } = mountList();
    await wrapper.setProps({
      groups: [
        {
          ...group,
          tasks: [taskFixture('task-a', 'A のタスク'), taskFixture('task-b', 'B のタスク')],
          total: 30,
          hasMore: true,
          oldestFirst: false,
        },
      ],
    });
    await nextTick();

    const html = wrapper.html();
    expect(html.indexOf('もっと見る')).toBeGreaterThan(html.indexOf('B のタスク'));
  });

  it('ページの取得に失敗したら再試行を出す', async () => {
    const retry = vi.fn();
    const { wrapper } = mountList();
    await wrapper.setProps({ groups: [{ ...group, isError: true, retry }] });
    await nextTick();

    const button = wrapper.findAll('button').find((b) => b.text() === '再試行');
    expect(button).toBeDefined();
    await button!.trigger('click');
    expect(retry).toHaveBeenCalledTimes(1);
  });

  it('作成の失敗をグループの下に出す', async () => {
    const { wrapper } = mountList();
    await wrapper.setProps({ createErrors: { 'status-todo': 'タスクを作成できませんでした' } });
    await nextTick();

    expect(wrapper.text()).toContain('タスクを作成できませんでした');
  });

  // 追加行は v-for の内側にあるので、template ref が配列で入る。素の `.$el` を読むと
  // 常に undefined になり、型検査も既存テストも通ったままフォーカスだけが効かなくなる
  it('追加行を開くとタイトル欄にフォーカスが当たる', async () => {
    const { wrapper } = mountList();
    await openAddRow(wrapper);

    const input = wrapper.get<HTMLInputElement>('input[aria-label="Todo にタスクを追加"]');
    expect(document.activeElement).toBe(input.element);
  });
});

describe('TaskGroupedList のサブタスクの初期展開', () => {
  const BranchStub = defineComponent({
    name: 'TaskSubtaskBranch',
    props: { parentTask: { type: Object, required: true }, implicit: Boolean },
    emits: ['empty', 'collapse'],
    template:
      '<div data-subtask-branch :data-task-id="parentTask.id" :data-implicit="String(implicit)" />',
  });

  const twoRows: TaskGroup[] = [
    { ...group, tasks: [taskFixture('task-a', 'A'), taskFixture('task-b', 'B')], total: 2 },
  ];

  type Wrapper = ReturnType<typeof mountList>['wrapper'];

  function mountExpanded(expandSubtasks: boolean | undefined) {
    return mountList(undefined, twoRows, [status], {
      expandSubtasks,
      stubs: { TaskSubtaskBranch: BranchStub },
    }).wrapper;
  }

  function branchOf(wrapper: Wrapper, id: string) {
    return wrapper.find(`[data-subtask-branch][data-task-id="${id}"]`);
  }

  /** index 番目の親行の矢印（展開中は「折りたたむ」、閉じていれば「展開」）。 */
  function arrowOf(wrapper: Wrapper, index: number) {
    return sectionOf(wrapper, 'Todo')
      .findAll('button[aria-expanded]')
      .filter((b) => b.attributes('aria-label')?.startsWith('サブタスクを'))[index];
  }

  it('既定（prop 無し）では枝を描かず、矢印は閉じている', () => {
    const wrapper = mountExpanded(undefined);

    expect(wrapper.findAll('[data-subtask-branch]')).toHaveLength(0);
    expect(arrowOf(wrapper, 0).attributes('aria-expanded')).toBe('false');
  });

  it('有効にすると、すべての親行の下に暗黙の枝を描き、矢印は開いている', () => {
    const wrapper = mountExpanded(true);

    expect(branchOf(wrapper, 'task-a').attributes('data-implicit')).toBe('true');
    expect(branchOf(wrapper, 'task-b').attributes('data-implicit')).toBe('true');
    expect(arrowOf(wrapper, 0).attributes('aria-expanded')).toBe('true');
    expect(arrowOf(wrapper, 1).attributes('aria-expanded')).toBe('true');
  });

  it('展開中に増えた行（もっと見る）にも枝を描く', async () => {
    const wrapper = mountExpanded(true);
    await wrapper.setProps({
      groups: [{ ...group, tasks: [...twoRows[0].tasks, taskFixture('task-c', 'C')], total: 3 }],
    });

    expect(branchOf(wrapper, 'task-c').exists()).toBe(true);
  });

  it('矢印で閉じた行だけが閉じ、もう一度押すと明示的な展開で戻る', async () => {
    const wrapper = mountExpanded(true);

    await arrowOf(wrapper, 0).trigger('click');
    expect(branchOf(wrapper, 'task-a').exists()).toBe(false);
    expect(branchOf(wrapper, 'task-b').exists()).toBe(true);
    expect(arrowOf(wrapper, 0).attributes('aria-expanded')).toBe('false');

    await arrowOf(wrapper, 0).trigger('click');
    expect(branchOf(wrapper, 'task-a').attributes('data-implicit')).toBe('false');
    expect(arrowOf(wrapper, 0).attributes('aria-expanded')).toBe('true');
  });

  it('設定を切り替えると上書きを捨て、全行が設定に従う', async () => {
    const wrapper = mountExpanded(true);
    await arrowOf(wrapper, 0).trigger('click'); // task-a を閉じる
    await arrowOf(wrapper, 0).trigger('click'); // task-a を明示的に開く
    await arrowOf(wrapper, 1).trigger('click'); // task-b を閉じる

    await wrapper.setProps({ expandSubtasks: false });
    expect(wrapper.findAll('[data-subtask-branch]')).toHaveLength(0);

    await wrapper.setProps({ expandSubtasks: true });
    expect(branchOf(wrapper, 'task-a').attributes('data-implicit')).toBe('true');
    expect(branchOf(wrapper, 'task-b').attributes('data-implicit')).toBe('true');
  });

  it('子が 0 件と報告された暗黙の枝は矢印だけ閉じて見せ、枝は残す', async () => {
    const wrapper = mountExpanded(true);
    const branchA = wrapper.findAllComponents(BranchStub)[0];

    branchA.vm.$emit('empty', true);
    await nextTick();
    expect(arrowOf(wrapper, 0).attributes('aria-expanded')).toBe('false');
    expect(branchOf(wrapper, 'task-a').exists()).toBe(true);

    branchA.vm.$emit('empty', false);
    await nextTick();
    expect(arrowOf(wrapper, 0).attributes('aria-expanded')).toBe('true');
  });

  it('空の暗黙の枝の矢印は 1 回で明示的な展開になる', async () => {
    const wrapper = mountExpanded(true);
    wrapper.findAllComponents(BranchStub)[0].vm.$emit('empty', true);
    await nextTick();

    await arrowOf(wrapper, 0).trigger('click');
    expect(branchOf(wrapper, 'task-a').attributes('data-implicit')).toBe('false');
    expect(arrowOf(wrapper, 0).attributes('aria-expanded')).toBe('true');
  });

  it('枝が collapse を返した行は閉じる', async () => {
    const wrapper = mountExpanded(true);
    await arrowOf(wrapper, 0).trigger('click');
    await arrowOf(wrapper, 0).trigger('click'); // 明示的に開く

    wrapper.findAllComponents(BranchStub)[0].vm.$emit('collapse');
    await nextTick();
    expect(branchOf(wrapper, 'task-a').exists()).toBe(false);
  });
});

describe('TaskGroupedList のサブタスクの子の有無（実物の枝）', () => {
  const oneRow: TaskGroup[] = [{ ...group, tasks: [taskFixture('task-a', 'A')], total: 1 }];
  const doingMoved: TaskGroup[] = [
    { ...group, tasks: [], total: 0 },
    { ...doingGroup, tasks: [taskFixture('task-a', 'A')], total: 1 },
  ];

  type Wrapper = ReturnType<typeof mountList>['wrapper'];

  function mountReal(expandSubtasks: boolean, groups: TaskGroup[] = oneRow) {
    return mountList(undefined, groups, [status, doingStatus], { expandSubtasks }).wrapper;
  }

  function arrowOf(wrapper: Wrapper, statusName = 'Todo') {
    return sectionOf(wrapper, statusName)
      .findAll('button[aria-expanded]')
      .find((b) => b.attributes('aria-label')?.startsWith('サブタスクを'))!;
  }

  beforeEach(() => {
    subtasksState.loading.value = false;
    subtasksState.pending.value = false;
    subtasksState.error.value = false;
    subtasksState.subtasks.value = [];
  });

  it('取得が成功して 0 件なら暗黙の枝の矢印は閉じて見える（対照: 子がいれば開いて見える）', async () => {
    const empty = mountReal(true);
    await flushPromises();
    expect(arrowOf(empty).attributes('aria-expanded')).toBe('false');

    subtasksState.subtasks.value = [taskFixture('child', '子')];
    const filled = mountReal(true);
    expect(arrowOf(filled).attributes('aria-expanded')).toBe('true');
  });

  it('取得に失敗した暗黙の枝は空として扱わず、矢印は開いたまま失敗を出す', async () => {
    subtasksState.error.value = true;
    const wrapper = mountReal(true);
    await flushPromises();

    expect(arrowOf(wrapper).attributes('aria-expanded')).toBe('true');
    expect(wrapper.text()).toContain('サブタスクを読み込めませんでした');
  });

  it('取得結果がまだ無い間は空として扱わず、届いて 0 件なら閉じて見える', async () => {
    subtasksState.pending.value = true;
    const wrapper = mountReal(true);
    await flushPromises();
    expect(arrowOf(wrapper).attributes('aria-expanded')).toBe('true');

    subtasksState.pending.value = false;
    await flushPromises();
    expect(arrowOf(wrapper).attributes('aria-expanded')).toBe('false');
  });

  it('子 0 件と記録された行が別のグループへ移っても、古い記録で矢印を閉じて見せない', async () => {
    const wrapper = mountReal(true);
    await flushPromises();
    expect(arrowOf(wrapper).attributes('aria-expanded')).toBe('false');

    // 移動先の枝は取り直し中（結果が無い）
    subtasksState.pending.value = true;
    await wrapper.setProps({ groups: doingMoved });
    await flushPromises();

    expect(arrowOf(wrapper, 'Doing').attributes('aria-expanded')).toBe('true');
  });

  it('子 0 件の行が前方のグループへ移っても（移動先が取得済みで即 0 件）、矢印は閉じて見え、1 回で作成欄が出る', async () => {
    const inDoing: TaskGroup[] = [
      { ...group, tasks: [], total: 0 },
      { ...doingGroup, tasks: [taskFixture('task-a', 'A')], total: 1 },
    ];
    const inTodo: TaskGroup[] = [
      { ...group, tasks: [taskFixture('task-a', 'A')], total: 1 },
      { ...doingGroup, tasks: [], total: 0 },
    ];
    // 結果 0 件・pending でない（キャッシュが残っていて移動先の枝は mount 直後に空と報告する）
    const wrapper = mountReal(true, inDoing);
    await flushPromises();
    expect(arrowOf(wrapper, 'Doing').attributes('aria-expanded')).toBe('false');

    await wrapper.setProps({ groups: inTodo });
    await flushPromises();
    expect(arrowOf(wrapper, 'Todo').attributes('aria-expanded')).toBe('false');

    await arrowOf(wrapper, 'Todo').trigger('click');
    await flushPromises();
    expect(arrowOf(wrapper, 'Todo').attributes('aria-expanded')).toBe('true');
    expect(wrapper.find('[data-subtask-branch]').exists()).toBe(true);
  });

  it('子 0 件の行が後方のグループへ移っても（移動先が即 0 件）、矢印は閉じて見え、1 回で作成欄が出る', async () => {
    const wrapper = mountReal(true);
    await flushPromises();
    expect(arrowOf(wrapper).attributes('aria-expanded')).toBe('false');

    await wrapper.setProps({ groups: doingMoved });
    await flushPromises();
    expect(arrowOf(wrapper, 'Doing').attributes('aria-expanded')).toBe('false');

    await arrowOf(wrapper, 'Doing').trigger('click');
    await flushPromises();
    expect(arrowOf(wrapper, 'Doing').attributes('aria-expanded')).toBe('true');
    expect(wrapper.find('[data-subtask-branch]').exists()).toBe(true);
  });

  it('手動で開いた子 0 件の行は、設定を有効にしても矢印を 1 回で作成欄まで開ける', async () => {
    const wrapper = mountReal(false);
    // 手動で開く。明示的な展開なので 0 件でも作成欄を出す
    await arrowOf(wrapper).trigger('click');
    await flushPromises();
    expect(arrowOf(wrapper).attributes('aria-expanded')).toBe('true');
    expect(wrapper.find('[data-subtask-branch]').exists()).toBe(true);

    // 設定を有効にする。上書きは捨てられ、枝は暗黙になる（0 件なので何も描かず、矢印は閉じて見える）
    await wrapper.setProps({ expandSubtasks: true });
    await flushPromises();
    expect(arrowOf(wrapper).attributes('aria-expanded')).toBe('false');
    expect(wrapper.find('[data-subtask-branch]').exists()).toBe(false);

    // 1 回で明示的な展開になり、作成欄が出る
    await arrowOf(wrapper).trigger('click');
    await flushPromises();
    expect(arrowOf(wrapper).attributes('aria-expanded')).toBe('true');
    expect(wrapper.find('[data-subtask-branch]').exists()).toBe(true);
  });

  it('子がいる行は設定を有効にしても矢印が開いたまま（対照）', async () => {
    subtasksState.subtasks.value = [taskFixture('child', '子')];
    const wrapper = mountReal(false);
    await arrowOf(wrapper).trigger('click');
    await wrapper.setProps({ expandSubtasks: true });
    await flushPromises();

    expect(arrowOf(wrapper).attributes('aria-expanded')).toBe('true');
  });
});
