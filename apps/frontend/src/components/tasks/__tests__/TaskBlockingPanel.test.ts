import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { enableAutoUnmount, mount, type VueWrapper } from '@vue/test-utils';
import { defineComponent, h, nextTick, onMounted } from 'vue';

type Entry = {
  id: string;
  relation_id: string;
  seq_id: number;
  title: string;
  status_id: string;
  progress_pct: number;
  planned_start_on?: string | null;
  soft_deadline?: string | null;
  hard_deadline?: string | null;
};

const state = vi.hoisted(() => ({
  loading: { value: false },
  error: { value: false },
  blocks: { value: [] as Entry[] },
  blockedBy: { value: [] as Entry[] },
  mutating: { value: false },
  mutationError: { value: null as string | null },
  addRelation: vi.fn(async () => true),
  removeRelation: vi.fn(async () => true),
  refetch: vi.fn(),
}));
// 図（wasm）は読まない。渡した中身と、読めなかったときの扱いだけを見る
const gantt = vi.hoisted(() => ({ fail: false }));

vi.mock('@/composables/useTaskBlocking', () => ({
  useTaskBlocking: () => state,
}));
vi.mock('@koyori-app/arc-vue', () => ({
  GanttChart: defineComponent({
    name: 'GanttChart',
    props: [
      'tasks',
      'rows',
      'deps',
      'view',
      'today',
      'palette',
      'rowHeight',
      'headerHeight',
      'rowLabels',
      'legend',
      'percentLabels',
    ],
    setup() {
      onMounted(() => {
        if (gantt.fail) throw new Error('wasm を読めない');
      });
      return () => h('div', { 'data-gantt-stub': '' });
    },
  }),
}));
vi.mock('@koyori-app/arc-vue/style.css', () => ({}));

import TaskBlockingPanel from '@/components/tasks/TaskBlockingPanel.vue';
import TaskParentPicker from '@/components/tasks/TaskParentPicker.vue';

enableAutoUnmount(afterEach);

const statuses = [
  { id: 'todo', name: '未着手', color: '#aaa', position: 0, is_done_state: false },
  { id: 'done', name: '完了', color: '#0a0', position: 1, is_done_state: true },
] as never[];

function entry(id: string, seq: number, status = 'todo', dates: Partial<Entry> = {}): Entry {
  return {
    id,
    relation_id: `rel-${id}`,
    seq_id: seq,
    title: `タスク${seq}`,
    status_id: status,
    progress_pct: 0,
    ...dates,
  };
}

const self = {
  id: 'task-id',
  seq_id: 1,
  title: 'このタスクの題',
  status_id: 'todo',
  progress_pct: 0,
  planned_start_on: '2026-11-16',
  soft_deadline: null,
  hard_deadline: '2026-11-24T00:00:00.000Z',
};

function mountPanel() {
  return mount(TaskBlockingPanel, {
    props: {
      tenantId: 'tenant-id',
      projectId: 'project-id',
      taskId: 'TASK-1',
      taskUuid: 'task-id',
      task: self as never,
      statuses,
      projectKey: 'TASK',
      tenantDisplayId: 'acme',
    },
    global: { plugins: [[VueQueryPlugin, { queryClient: new QueryClient() }]] },
    attachTo: document.body,
  });
}

function buttonByText(wrapper: VueWrapper, text: string) {
  return wrapper.findAll('button').find((button) => button.text().includes(text));
}
function menuItem(wrapper: VueWrapper, text: string) {
  return wrapper.findAll('[role="menuitem"]').find((item) => item.text() === text)!;
}
const labelRows = (wrapper: VueWrapper) => wrapper.findAll('[data-blocking-role]');

describe('TaskBlockingPanel', () => {
  beforeEach(() => {
    state.loading.value = false;
    state.error.value = false;
    state.blocks.value = [];
    state.blockedBy.value = [];
    state.mutating.value = false;
    state.mutationError.value = null;
    state.addRelation.mockClear();
    state.removeRelation.mockClear();
    state.refetch.mockClear();
    gantt.fail = false;
  });

  it('ラベルは 待たせている相手 → このタスク → 待っている相手 の順で、相手を押すと open を emit する', async () => {
    state.blockedBy.value = [entry('a', 2)];
    state.blocks.value = [entry('b', 3)];
    const wrapper = mountPanel();

    const rows = labelRows(wrapper);
    expect(rows.map((row) => row.attributes('data-blocking-role'))).toEqual([
      'blocked_by',
      'self',
      'blocks',
    ]);
    expect(rows[0].text()).toContain('TASK-2');
    expect(rows[1].text()).toContain('TASK-1');
    expect(rows[2].text()).toContain('TASK-3');
    // 向きごとの見出しと一覧は無い
    expect(wrapper.find('h3').exists()).toBe(false);

    await rows[2].get('button[data-task-row-card]').trigger('click');
    expect(wrapper.emitted('open')?.[0]).toEqual([state.blocks.value[0]]);
  });

  it('このタスクの行は太字で、押せる形にしない', () => {
    state.blockedBy.value = [entry('a', 2)];
    const self = labelRows(mountPanel())[1];

    expect(self.get('.font-semibold').text()).toBe('このタスクの題');
    expect(self.find('button').exists()).toBe(false);
    expect(self.find('[data-task-row-card]').exists()).toBe(false);
  });

  it('ラベルの列（208px）と行の高さ（32px）を図と揃える', () => {
    state.blockedBy.value = [entry('a', 2)];
    const wrapper = mountPanel();

    const list = wrapper.get('[data-task-blocking-chart] ul');
    expect(list.classes()).toContain('w-52');
    expect(list.attributes('style')).toContain('padding-top: 32px');
    for (const row of labelRows(wrapper)) expect(row.attributes('style')).toContain('height: 32px');
    const chart = wrapper.getComponent({ name: 'GanttChart' });
    expect(chart.props('rowHeight')).toBe(32);
    expect(chart.props('headerHeight')).toBe(32);
    expect(chart.props('rowLabels')).toBe('hidden');
    expect(chart.props('legend')).toBe('hidden');
  });

  it('図に行・依存・期間・今日を渡す', () => {
    state.blockedBy.value = [entry('a', 2, 'done')];
    state.blocks.value = [entry('b', 3)];
    const chart = mountPanel().getComponent({ name: 'GanttChart' });

    expect((chart.props('tasks') as { id: string }[]).map((t) => t.id)).toEqual([
      'a',
      'task-id',
      'b',
    ]);
    expect(chart.props('rows')).toHaveLength(3);
    expect(chart.props('deps')).toEqual([
      { blocker_task_id: 'a', blocked_task_id: 'task-id', state: 'done' },
      { blocker_task_id: 'task-id', blocked_task_id: 'b', state: 'normal' },
    ]);
    expect(chart.props('today')).toMatch(/^\d{4}-\d{2}-\d{2}$/);
    expect(chart.props('view')).toEqual({
      from: expect.stringMatching(/^\d{4}-\d{2}-\d{2}$/),
      to: expect.stringMatching(/^\d{4}-\d{2}-\d{2}$/),
    });
  });

  it('日付の食い違いがあるときだけ、図の下に注記を出す', () => {
    state.blockedBy.value = [entry('a', 2, 'todo', { hard_deadline: '2026-11-10T00:00:00.000Z' })];
    expect(mountPanel().find('[data-task-blocking-notes]').exists()).toBe(false);

    state.blockedBy.value = [entry('a', 2, 'todo', { hard_deadline: '2026-11-21T00:00:00.000Z' })];
    const notes = mountPanel().get('[data-task-blocking-notes]');
    expect(notes.text()).toBe('点線：TASK-2 の期限（11/21）が、このタスクの開始（11/16）より後');
  });

  it('ラベルのホバーカードの器を 1 つ置く（相手だけが起点）', async () => {
    state.blockedBy.value = [entry('a', 2)];
    state.blocks.value = [entry('b', 3)];
    const wrapper = mountPanel();
    await nextTick();

    const cards = wrapper.findAllComponents({ name: 'TaskRowHoverCard' });
    expect(cards).toHaveLength(1);
    expect((cards[0].props('tasks') as Entry[]).map((t) => t.id)).toEqual(['a', 'b']);
    expect(cards[0].props('container')).toBe(wrapper.get('[data-task-blocking-chart] ul').element);
    expect(
      wrapper.findAll('[data-task-row-card]').map((el) => el.attributes('data-task-id')),
    ).toEqual(['a', 'b']);
  });

  it('図を読めなくても節は落ちず、ラベルの列を残して案内を出す', async () => {
    gantt.fail = true;
    state.blockedBy.value = [entry('a', 2)];
    const wrapper = mountPanel();
    await nextTick();

    expect(wrapper.text()).toContain('図を表示できませんでした');
    expect(wrapper.find('[data-gantt-stub]').exists()).toBe(false);
    expect(labelRows(wrapper)).toHaveLength(2);
  });

  it.each([
    [0, false],
    [1, true],
    [2, true],
  ])('未完了のブロッカーが %i 件のとき件数表示は %s', (open, shown) => {
    state.blockedBy.value = [
      ...Array.from({ length: open }, (_, i) => entry(`o${i}`, 10 + i)),
      entry('d', 99, 'done'),
    ];
    const wrapper = mountPanel();
    const count = wrapper.find('[data-task-blocking-open-count]');
    expect(count.exists()).toBe(shown);
    if (shown) expect(count.text()).toContain(`${open}`);
  });

  it('完了済みの相手は muted で描く', () => {
    state.blockedBy.value = [entry('o', 2), entry('d', 3, 'done')];
    const [open, done] = labelRows(mountPanel());
    expect(open.get('button[data-task-row-card]').classes()).not.toContain('text-muted-foreground');
    expect(done.get('button[data-task-row-card]').classes()).toContain('text-muted-foreground');
  });

  it('関係が無いときは空の文言を出し、図は出さない', () => {
    const wrapper = mountPanel();
    expect(wrapper.text()).toContain('ブロック関係はありません');
    expect(wrapper.find('[data-task-blocking-chart]').exists()).toBe(false);
  });

  it('追加の入口は見出しの右の 1 つで、押してから向きを選ぶ', async () => {
    state.blockedBy.value = [entry('a', 2)];
    const wrapper = mountPanel();

    const addButtons = wrapper.findAll('button').filter((b) => b.text() === '関係を追加');
    expect(addButtons).toHaveLength(1);
    expect(wrapper.get('h2').element.parentElement!.contains(addButtons[0].element)).toBe(true);
    expect(wrapper.findComponent(TaskParentPicker).exists()).toBe(false);

    await menuItem(wrapper, 'ブロックするタスクを追加').trigger('click');
    expect(wrapper.find('input[aria-label="ブロックするタスクを検索"]').exists()).toBe(true);
  });

  it('もう一方の向きを選ぶと開いている picker を切り替える', async () => {
    const wrapper = mountPanel();
    await menuItem(wrapper, 'ブロックするタスクを追加').trigger('click');
    await menuItem(wrapper, 'ブロックされるタスクを追加').trigger('click');
    expect(wrapper.findAllComponents(TaskParentPicker)).toHaveLength(1);
    expect(wrapper.find('input[aria-label="ブロックされるタスクを検索"]').exists()).toBe(true);
    expect(wrapper.find('input[aria-label="ブロックするタスクを検索"]').exists()).toBe(false);
  });

  it('候補の除外に自分・blocks・blocked_by の全 id が入る', async () => {
    state.blocks.value = [entry('b', 3)];
    state.blockedBy.value = [entry('a', 2)];
    const wrapper = mountPanel();
    await menuItem(wrapper, 'ブロックするタスクを追加').trigger('click');
    expect(wrapper.getComponent(TaskParentPicker).props('excludeTaskIds')).toEqual([
      'task-id',
      'b',
      'a',
    ]);
  });

  it('候補を選ぶと addRelation(type, id) を呼び picker を閉じる', async () => {
    const wrapper = mountPanel();
    await menuItem(wrapper, 'ブロックするタスクを追加').trigger('click');
    wrapper.getComponent(TaskParentPicker).vm.$emit('select', 'target-id');
    await wrapper.vm.$nextTick();
    expect(state.addRelation).toHaveBeenCalledWith('blocks', 'target-id');
    expect(wrapper.findComponent(TaskParentPicker).exists()).toBe(false);

    await menuItem(wrapper, 'ブロックされるタスクを追加').trigger('click');
    wrapper.getComponent(TaskParentPicker).vm.$emit('select', 'target-2');
    expect(state.addRelation).toHaveBeenLastCalledWith('blocked_by', 'target-2');
  });

  it('関係を外す操作はラベルの行のメニューに残り、removeRelation(relation_id) を呼ぶ', async () => {
    state.blocks.value = [entry('b', 3)];
    const wrapper = mountPanel();

    const row = labelRows(wrapper)[1];
    expect(row.find('button[aria-label="TASK-3 との関係の操作"]').exists()).toBe(true);
    await row
      .findAll('[role="menuitem"]')
      .find((item) => item.text() === '関係を外す')!
      .trigger('click');
    expect(state.removeRelation).toHaveBeenCalledWith('rel-b');
  });

  it('mutating 中は追加・外す・候補が disabled', async () => {
    state.blocks.value = [entry('b', 3)];
    const wrapper = mountPanel();
    await menuItem(wrapper, 'ブロックするタスクを追加').trigger('click');
    state.mutating.value = true;
    await wrapper.vm.$forceUpdate();
    await wrapper.vm.$nextTick();

    expect(
      wrapper.get('button[aria-label="TASK-3 との関係の操作"]').attributes('disabled'),
    ).toBeDefined();
    expect(buttonByText(wrapper, '関係を追加')!.attributes('disabled')).toBeDefined();
    expect(wrapper.getComponent(TaskParentPicker).props('disabled')).toBe(true);
  });

  it('mutationError を role=alert で出す', () => {
    state.mutationError.value = '循環する関係や同じ関係は設定できません';
    const wrapper = mountPanel();
    expect(wrapper.get('[role="alert"]').text()).toBe('循環する関係や同じ関係は設定できません');
  });

  it('読み込み失敗で再試行を出す', async () => {
    state.error.value = true;
    const wrapper = mountPanel();
    expect(wrapper.text()).toContain('ブロック関係を読み込めませんでした');
    await buttonByText(wrapper, '再試行')!.trigger('click');
    expect(state.refetch).toHaveBeenCalled();
  });

  it('別のタスクへ移ったら picker を閉じる', async () => {
    const wrapper = mountPanel();
    await menuItem(wrapper, 'ブロックするタスクを追加').trigger('click');
    expect(wrapper.findComponent(TaskParentPicker).exists()).toBe(true);
    await wrapper.setProps({ taskUuid: 'other-task' });
    expect(wrapper.findComponent(TaskParentPicker).exists()).toBe(false);
  });
});
