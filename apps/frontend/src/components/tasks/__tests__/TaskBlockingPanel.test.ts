import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { enableAutoUnmount, mount, type VueWrapper } from '@vue/test-utils';

type Entry = { id: string; relation_id: string; seq_id: number; title: string; status_id: string };

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

vi.mock('@/composables/useTaskBlocking', () => ({
  useTaskBlocking: () => state,
}));

import TaskBlockingPanel from '@/components/tasks/TaskBlockingPanel.vue';
import TaskParentPicker from '@/components/tasks/TaskParentPicker.vue';

enableAutoUnmount(afterEach);

const statuses = [
  { id: 'todo', name: '未着手', color: '#aaa', is_done_state: false },
  { id: 'done', name: '完了', color: '#0a0', is_done_state: true },
] as never[];

function entry(id: string, seq: number, status = 'todo'): Entry {
  return { id, relation_id: `rel-${id}`, seq_id: seq, title: `タスク${seq}`, status_id: status };
}

function mountPanel() {
  return mount(TaskBlockingPanel, {
    props: {
      tenantId: 'tenant-id',
      projectId: 'project-id',
      taskId: 'TASK-1',
      taskUuid: 'task-id',
      statuses,
      projectKey: 'TASK',
    },
    global: { plugins: [[VueQueryPlugin, { queryClient: new QueryClient() }]] },
  });
}

function buttonByText(wrapper: VueWrapper, text: string) {
  return wrapper.findAll('button').find((button) => button.text().includes(text));
}

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
  });

  it('blocked_by を先、blocks を後に出し、行を押すと open を emit する', async () => {
    state.blockedBy.value = [entry('a', 2)];
    state.blocks.value = [entry('b', 3)];
    const wrapper = mountPanel();

    const rows = wrapper.findAll('li');
    expect(rows[0].text()).toContain('TASK-2');
    expect(rows[1].text()).toContain('TASK-3');
    const headings = wrapper.findAll('h3').map((h) => h.text());
    expect(headings).toEqual(['ブロックされている', 'ブロックしている']);

    await rows[1].get('button').trigger('click');
    expect(wrapper.emitted('open')?.[0]).toEqual([state.blocks.value[0]]);
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
    const wrapper = mountPanel();
    const [open, done] = wrapper.findAll('li');
    expect(open.classes()).not.toContain('text-muted-foreground');
    expect(done.classes()).toContain('text-muted-foreground');
  });

  it('空のときはそれぞれの空文言を出す', () => {
    const wrapper = mountPanel();
    expect(wrapper.text()).toContain('ブロックされているタスクはありません');
    expect(wrapper.text()).toContain('ブロックしているタスクはありません');
  });

  it('「ブロックするタスクを追加」で picker が開き、aria-label がブロック用になる', async () => {
    const wrapper = mountPanel();
    await buttonByText(wrapper, 'ブロックするタスクを追加')!.trigger('click');
    expect(wrapper.find('input[aria-label="ブロックするタスクを検索"]').exists()).toBe(true);
  });

  it('もう一方の追加を押すと開いている picker を切り替える', async () => {
    const wrapper = mountPanel();
    await buttonByText(wrapper, 'ブロックするタスクを追加')!.trigger('click');
    await buttonByText(wrapper, 'ブロックされるタスクを追加')!.trigger('click');
    expect(wrapper.findAllComponents(TaskParentPicker)).toHaveLength(1);
    expect(wrapper.find('input[aria-label="ブロックされるタスクを検索"]').exists()).toBe(true);
    expect(wrapper.find('input[aria-label="ブロックするタスクを検索"]').exists()).toBe(false);
  });

  it('候補の除外に自分・blocks・blocked_by の全 id が入る', async () => {
    state.blocks.value = [entry('b', 3)];
    state.blockedBy.value = [entry('a', 2)];
    const wrapper = mountPanel();
    await buttonByText(wrapper, 'ブロックするタスクを追加')!.trigger('click');
    expect(wrapper.getComponent(TaskParentPicker).props('excludeTaskIds')).toEqual([
      'task-id',
      'b',
      'a',
    ]);
  });

  it('候補を選ぶと addRelation(type, id) を呼び picker を閉じる', async () => {
    const wrapper = mountPanel();
    await buttonByText(wrapper, 'ブロックするタスクを追加')!.trigger('click');
    wrapper.getComponent(TaskParentPicker).vm.$emit('select', 'target-id');
    await wrapper.vm.$nextTick();
    expect(state.addRelation).toHaveBeenCalledWith('blocks', 'target-id');
    expect(wrapper.findComponent(TaskParentPicker).exists()).toBe(false);

    await buttonByText(wrapper, 'ブロックされるタスクを追加')!.trigger('click');
    wrapper.getComponent(TaskParentPicker).vm.$emit('select', 'target-2');
    expect(state.addRelation).toHaveBeenLastCalledWith('blocked_by', 'target-2');
  });

  it('外すボタンは removeRelation(relation_id) を呼ぶ', async () => {
    state.blocks.value = [entry('b', 3)];
    const wrapper = mountPanel();
    await wrapper.get('button[aria-label="TASK-3 との関係を外す"]').trigger('click');
    expect(state.removeRelation).toHaveBeenCalledWith('rel-b');
  });

  it('mutating 中は追加・外す・候補が disabled', async () => {
    state.blocks.value = [entry('b', 3)];
    const wrapper = mountPanel();
    await buttonByText(wrapper, 'ブロックするタスクを追加')!.trigger('click');
    state.mutating.value = true;
    await wrapper.vm.$forceUpdate();
    await wrapper.vm.$nextTick();

    expect(
      wrapper.get('button[aria-label="TASK-3 との関係を外す"]').attributes('disabled'),
    ).toBeDefined();
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
    await buttonByText(wrapper, 'ブロックするタスクを追加')!.trigger('click');
    expect(wrapper.findComponent(TaskParentPicker).exists()).toBe(true);
    await wrapper.setProps({ taskUuid: 'other-task' });
    expect(wrapper.findComponent(TaskParentPicker).exists()).toBe(false);
  });
});
