import { readFileSync } from 'node:fs';
import path from 'node:path';

import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { config, enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { QueryClient, useQuery, VueQueryPlugin } from '@tanstack/vue-query';
import { defineComponent, nextTick } from 'vue';
import type { components } from '@/generated/api';

/*
 * MarkdownEditor (CodeMirror) は happy-dom では起動できない。ここで見たいのは
 * 「TaskDetailHub が下書きを編集器へ渡し、編集中は KFM 表示を引っ込める」という
 * 器側の配線なので、編集器は textarea の代役に差し替える (代役の説明は stub 側)。
 * 実物の CodeMirror は実ブラウザで動く story (TaskDetail.stories.ts の説明編集) が見る。
 */
vi.mock('@/components/markdown/MarkdownEditor.vue', async () => ({
  default: (await import('@/components/markdown/__tests__/markdown-editor-stub')).default,
}));

import { ConfirmDialog } from '@koyori-app/ui-vue';
import TaskDetailHub from '../TaskDetailHub.vue';
import { PRIORITY_CONFIG } from '@/lib/task-display';
import { DEADLINE_ORDER_MESSAGE } from '@/lib/task-date';
import { renderDescription } from '@/lib/markup-renderer';

enableAutoUnmount(afterEach);

// 読み込み失敗の再試行（失敗した query の refetch）が QueryClient を使う
config.global.plugins = [[VueQueryPlugin, { queryClient: new QueryClient() }]];

type TaskDetail = components['schemas']['TaskDetailResponse'];

const task: TaskDetail = {
  assignees: [],
  created_at: '2026-07-16T00:00:00Z',
  custom_field_values: [],
  reactions: [],
  attachments: [],
  id: 'task-id',
  is_archived: false,
  labels: [],
  priority: 'Medium',
  progress_pct: 30,
  project_id: 'project-id',
  seq_id: 1,
  status_id: 'status-id',
  title: 'Test task',
  updated_at: '2026-07-16T00:00:00Z',
};

type StatusOption = components['schemas']['ProjectStatusResponse'];

/** ワークフロー順（position）の 3 段。「次へ」と「完了にする」の検証で使う。 */
const statuses: StatusOption[] = [
  {
    id: 'status-id',
    name: 'Todo',
    color: '#94a3b8',
    position: 0,
    is_default: true,
    is_done_state: false,
    is_default_done: false,
    project_id: 'project-id',
    created_at: '2026-07-16T00:00:00Z',
  },
  {
    id: 'status-progress',
    name: 'In Progress',
    color: '#3b82f6',
    position: 1,
    is_default: false,
    is_done_state: false,
    is_default_done: false,
    project_id: 'project-id',
    created_at: '2026-07-16T00:00:00Z',
  },
  {
    id: 'status-done',
    name: 'Done',
    color: '#22c55e',
    position: 2,
    is_default: false,
    is_done_state: true,
    is_default_done: true,
    project_id: 'project-id',
    created_at: '2026-07-16T00:00:00Z',
  },
];

const bugLabel: components['schemas']['LabelResponse'] = {
  id: 'label-bug',
  name: 'bug',
  description: '',
  color: '#e11d48',
  icon_url: null,
  project_id: 'project-id',
};

const featureLabel: components['schemas']['LabelResponse'] = {
  id: 'label-feature',
  name: 'feature',
  description: '',
  color: '#3b82f6',
  icon_url: null,
  project_id: 'project-id',
};

type TaskTypeOption = components['schemas']['TaskTypeResponse'];

const taskTypes: TaskTypeOption[] = [
  { id: 'type-task', project_id: 'project-id', name: 'タスク', color: '#64748b', position: 0 },
  { id: 'type-bug', project_id: 'project-id', name: 'バグ', color: '#ef4444', position: 1 },
  { id: 'type-feature', project_id: 'project-id', name: '機能', color: '#3b82f6', position: 2 },
];

describe('TaskDetailHub の種別', () => {
  function mountWithTaskType(extra: Record<string, unknown> = {}) {
    return mount(TaskDetailHub, {
      props: {
        task: { ...task, task_type: taskTypes[1] },
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
        projectTaskTypes: taskTypes,
        ...extra,
      },
    });
  }

  function typeItems(wrapper: ReturnType<typeof mountWithTaskType>) {
    return wrapper.get('[role="menu"][aria-label="種別"]').findAll('[role="menuitemradio"]');
  }

  it('種別があれば名前と色の点を出し、固定の「タスク」チップは出さない', () => {
    const wrapper = mountWithTaskType();

    const trigger = wrapper.get('button[aria-label="種別"]');
    // 引き金は選んだ種別の名前だけ。以前の固定表示（アイコン + 「タスク」）ではない
    expect(trigger.text()).toBe('バグ');
    expect(trigger.get('span[style*="background-color"]').attributes('style')).toContain('#ef4444');
  });

  it('種別が無ければ「種別」と出す', () => {
    const wrapper = mountWithTaskType({ task });

    const trigger = wrapper.get('button[aria-label="種別"]');
    expect(trigger.text()).toBe('種別');
    expect(trigger.find('span[style*="background-color"]').exists()).toBe(false);
  });

  it('選択肢は「種別なし」と各種別で、選択中の種別に印が付く', () => {
    const wrapper = mountWithTaskType();

    const items = typeItems(wrapper);
    expect(items.map((item) => item.text())).toEqual(['種別なし', 'タスク', 'バグ', '機能']);
    expect(items.map((item) => item.attributes('aria-checked'))).toEqual([
      'false',
      'false',
      'true',
      'false',
    ]);
  });

  it('種別が無いタスクでは「種別なし」に印が付く', () => {
    const wrapper = mountWithTaskType({ task });

    expect(typeItems(wrapper).map((item) => item.attributes('aria-checked'))).toEqual([
      'true',
      'false',
      'false',
      'false',
    ]);
  });

  it('種別を選ぶと ID を、「種別なし」を選ぶと null を change:task_type で emit する', async () => {
    const wrapper = mountWithTaskType();

    await typeItems(wrapper)[3]!.trigger('click');
    await typeItems(wrapper)[0]!.trigger('click');

    expect(wrapper.emitted('change:task_type')).toEqual([['type-feature'], [null]]);
  });

  it('更新中は種別を触れない', () => {
    const wrapper = mountWithTaskType({ taskTypeUpdating: true });

    expect(wrapper.get('button[aria-label="種別"]').attributes('disabled')).toBeDefined();
    expect(typeItems(wrapper).every((item) => item.attributes('aria-disabled') === 'true')).toBe(
      true,
    );
  });

  it('更新に失敗したら理由を出し、「保存しました」は出さない', () => {
    const wrapper = mountWithTaskType({
      taskTypeError: '種別の更新に失敗しました',
      savedField: 'task_type',
    });

    expect(wrapper.text()).toContain('種別の更新に失敗しました');
    expect(wrapper.text()).not.toContain('保存しました');
  });

  it('保存が確定したら「保存しました」を出す', () => {
    const wrapper = mountWithTaskType({ savedField: 'task_type' });

    const notices = wrapper.findAll('[role="status"]');
    expect(notices).toHaveLength(1);
    expect(notices[0]!.text()).toBe('保存しました');
  });

  it('候補を渡されなければメニューを出さず、種別の名前だけを表示する', () => {
    const wrapper = mountWithTaskType({ projectTaskTypes: undefined });

    expect(wrapper.find('button[aria-label="種別"]').exists()).toBe(false);
    expect(wrapper.find('[role="menu"][aria-label="種別"]').exists()).toBe(false);
    expect(wrapper.text()).toContain('バグ');
  });

  it('候補が 0 件で種別も無ければ、作成ダイアログと同じくチップを出さない', () => {
    const wrapper = mountWithTaskType({ task, projectTaskTypes: [] });

    expect(wrapper.find('button[aria-label="種別"]').exists()).toBe(false);
  });

  it('候補が 0 件でも種別が付いていれば、名前だけを表示する', () => {
    const wrapper = mountWithTaskType({ projectTaskTypes: [] });

    expect(wrapper.find('button[aria-label="種別"]').exists()).toBe(false);
    expect(wrapper.text()).toContain('バグ');
  });

  it('候補を渡されず種別も無ければ何も出さない', () => {
    const wrapper = mountWithTaskType({ task, projectTaskTypes: undefined });

    expect(wrapper.find('button[aria-label="種別"]').exists()).toBe(false);
  });
});

describe('TaskDetailHub', () => {
  /**
   * 優先度は表示だけで変更できなかった（作成時にしか決められなかった）。
   * 選び直しを emit する。ステータスと同じピル + メニュー（参照デザイン）なので、
   * native select ではなくメニュー項目を押して確かめる。項目は閉じていても DOM にある。
   */
  function mountWithPriorityMenu(extra: Record<string, unknown> = {}) {
    return mount(TaskDetailHub, {
      props: { task, projectKey: 'TEST', statuses: [], statusId: task.status_id, ...extra },
    });
  }

  function priorityItems(wrapper: ReturnType<typeof mountWithPriorityMenu>) {
    return wrapper.get('[role="menu"][aria-label="優先度"]').findAll('[role="menuitemradio"]');
  }

  it('優先度を選び直すと change:priority を emit する', async () => {
    const wrapper = mountWithPriorityMenu();

    expect(wrapper.get('button[aria-label="優先度"]').text()).toContain(
      PRIORITY_CONFIG.Medium.label,
    );

    const items = priorityItems(wrapper);
    const high = items.find((item) => item.text().includes(PRIORITY_CONFIG.High.label));
    expect(high).toBeDefined();
    await high!.trigger('click');

    expect(wrapper.emitted('change:priority')).toEqual([['High']]);
  });

  it('優先度の色は旗だけに載せ、文字には載せない', () => {
    const wrapper = mountWithPriorityMenu();

    expect(wrapper.html()).not.toContain('--priority-color');
    const flag = wrapper.get('button[aria-label="優先度"] svg');
    expect(flag.attributes('style')).toContain('color: var(--priority-medium)');
  });

  it('本文のスクロール領域で切れないよう、メニューはどれもトップレイヤーに出す', () => {
    const wrapper = mountWithPriorityMenu({ statuses, projectLabels: [featureLabel] });

    const triggers = wrapper.findAll('button[aria-haspopup="menu"]');
    expect(triggers.length).toBeGreaterThanOrEqual(3);
    for (const trigger of triggers) {
      expect(trigger.element.closest('[data-menu-top-layer]'), trigger.text()).not.toBeNull();
    }
  });

  it('優先度の選択肢は表示できる優先度をすべて出す', () => {
    const wrapper = mountWithPriorityMenu();

    const labels = priorityItems(wrapper).map((item) => item.text());
    expect(labels).toEqual(Object.values(PRIORITY_CONFIG).map((config) => config.label));
  });

  it('更新中は優先度を触れない', () => {
    const wrapper = mountWithPriorityMenu({ priorityUpdating: true });

    expect(wrapper.get('button[aria-label="優先度"]').attributes('disabled')).toBeDefined();
    expect(
      priorityItems(wrapper).every((item) => item.attributes('aria-disabled') === 'true'),
    ).toBe(true);
  });

  // TASK-484: スペースのない URL が右端で切れた。flex の子は min-w-0 が無いと折り返さない
  it('タイトルは幅に収まらなくても折り返す', () => {
    const classes = mountWithPriorityMenu().get('h1').classes();

    expect(classes).toContain('min-w-0');
    expect(classes).toContain('break-words');
  });

  /** happy-dom の既定に clipboard があるとは限らないので、都度 stub を据える */
  function stubClipboard(writeText: (text: string) => Promise<void>) {
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
  }

  afterEach(() => {
    delete (navigator as { clipboard?: unknown }).clipboard;
    vi.useRealTimers();
  });

  it('タスクIDのボタンを押すと clipboard へ TEST-1 を写す', async () => {
    const writeText = vi.fn(async () => undefined);
    stubClipboard(writeText);

    const wrapper = mount(TaskDetailHub, {
      props: { task, projectKey: 'TEST', statuses: [], statusId: task.status_id },
    });

    await wrapper.get('button[aria-label="タスクID TEST-1 をコピー"]').trigger('click');
    await nextTick();

    expect(writeText).toHaveBeenCalledWith('TEST-1');
    expect(wrapper.find('button[aria-label="タスクIDをコピーしました"]').exists()).toBe(true);
  });

  it('コピーに失敗したら握り潰さず失敗を示す', async () => {
    stubClipboard(vi.fn(async () => Promise.reject(new Error('denied'))));

    const wrapper = mount(TaskDetailHub, {
      props: { task, projectKey: 'TEST', statuses: [], statusId: task.status_id },
    });

    await wrapper.get('button[aria-label="タスクID TEST-1 をコピー"]').trigger('click');
    await nextTick();

    expect(wrapper.find('button[aria-label="タスクIDをコピーできませんでした"]').exists()).toBe(
      true,
    );
  });

  /*
   * 非 secure context では navigator.clipboard そのものが無い。いまは writeText の参照が
   * 同期 TypeError になって catch に入るが、`navigator.clipboard?.writeText(...)` のような
   * 整理を入れると undefined を await して resolve し、写していないのに成功と出る。
   */
  it('clipboard が無い環境では成功と誤表示せず失敗を示す', async () => {
    // happy-dom は clipboard を自前で持っていて delete では消えないので、undefined を据える
    // （afterEach の delete はこの上書きを外して既定へ戻す）
    Object.defineProperty(navigator, 'clipboard', { value: undefined, configurable: true });
    expect((navigator as { clipboard?: unknown }).clipboard).toBeUndefined();

    const wrapper = mount(TaskDetailHub, {
      props: { task, projectKey: 'TEST', statuses: [], statusId: task.status_id },
    });

    await wrapper.get('button[aria-label="タスクID TEST-1 をコピー"]').trigger('click');
    await nextTick();

    expect(wrapper.find('button[aria-label="タスクIDをコピーしました"]').exists()).toBe(false);
    expect(wrapper.find('button[aria-label="タスクIDをコピーできませんでした"]').exists()).toBe(
      true,
    );
  });

  /*
   * 連続して押すと複数のコピーが同時に走る。応答が逆順に返ると、古い操作の結果が
   * 新しい操作の表示を上書きしうる。古い操作のタイマーが残っていると、最後に押してから
   * 2 秒経つ前に表示も消える。
   */
  it('連続コピーで応答が逆順に返っても、最後の操作の結果を最後まで出す', async () => {
    vi.useFakeTimers();
    const pending: { resolve: () => void; reject: (reason: Error) => void }[] = [];
    stubClipboard(
      vi.fn(
        () =>
          new Promise<void>((resolve, reject) => {
            pending.push({ resolve, reject });
          }),
      ),
    );

    const wrapper = mount(TaskDetailHub, {
      props: { task, projectKey: 'TEST', statuses: [], statusId: task.status_id },
    });
    const button = wrapper.get('button[aria-label="タスクID TEST-1 をコピー"]');

    await button.trigger('click');
    await button.trigger('click');
    expect(pending).toHaveLength(2);

    // 2 回目が先に成功し、1 回目の失敗があとから返る
    pending[1]!.resolve();
    await flushPromises();
    expect(wrapper.find('button[aria-label="タスクIDをコピーしました"]').exists()).toBe(true);

    pending[0]!.reject(new Error('denied'));
    await flushPromises();
    expect(wrapper.find('button[aria-label="タスクIDをコピーできませんでした"]').exists()).toBe(
      false,
    );
    expect(wrapper.find('button[aria-label="タスクIDをコピーしました"]').exists()).toBe(true);

    // 表示は最後の結果から 2 秒続く（古い操作のタイマーで早く消えない）
    await vi.advanceTimersByTimeAsync(1_900);
    expect(wrapper.find('button[aria-label="タスクIDをコピーしました"]').exists()).toBe(true);
    await vi.advanceTimersByTimeAsync(200);
    expect(wrapper.find('button[aria-label="タスクID TEST-1 をコピー"]').exists()).toBe(true);
  });

  it('優先度の更新に失敗したら理由を出す', () => {
    const wrapper = mount(TaskDetailHub, {
      props: {
        task,
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
        priorityError: '優先度の更新に失敗しました',
      },
    });

    expect(wrapper.text()).toContain('優先度の更新に失敗しました');
  });

  it('保存が確定したフィールドに「保存しました」を出す', () => {
    const wrapper = mount(TaskDetailHub, {
      props: {
        task,
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
        savedField: 'description',
      },
    });

    const notices = wrapper.findAll('[role="status"]');
    expect(notices).toHaveLength(1);
    expect(notices[0]!.text()).toBe('保存しました');
  });

  it.each(['status_id', 'priority', 'labels'] as const)(
    '%s の保存が確定したら「保存しました」を出す',
    (savedField) => {
      const wrapper = mount(TaskDetailHub, {
        props: { task, projectKey: 'TEST', statuses: [], statusId: task.status_id, savedField },
      });

      const notices = wrapper.findAll('[role="status"]');
      expect(notices).toHaveLength(1);
      expect(notices[0]!.text()).toBe('保存しました');
    },
  );

  it.each([
    ['status_id', 'statusError'],
    ['priority', 'priorityError'],
    ['labels', 'labelsError'],
  ] as const)('%s の失敗があれば「保存しました」は出さず失敗だけ出す', (savedField, errorProp) => {
    const wrapper = mount(TaskDetailHub, {
      props: {
        task,
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
        savedField,
        [errorProp]: '更新に失敗しました',
      },
    });

    expect(wrapper.text()).not.toContain('保存しました');
    expect(wrapper.text()).toContain('更新に失敗しました');
  });

  it('同じフィールドの失敗があれば「保存しました」は出さず失敗を出す', () => {
    const wrapper = mount(TaskDetailHub, {
      props: {
        task,
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
        savedField: 'description',
        fieldErrors: { description: '更新に失敗しました' },
      },
    });

    expect(wrapper.text()).not.toContain('保存しました');
    expect(wrapper.text()).toContain('更新に失敗しました');
  });

  it('emits the selected soft deadline through the SET path', async () => {
    const wrapper = mount(TaskDetailHub, {
      props: {
        task,
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
      },
    });

    await wrapper.get('button[aria-label="ソフト期限を編集"]').trigger('click');
    const deadlineInput = wrapper.get('input[aria-label="ソフト期限"]');
    await deadlineInput.setValue('2026-08-23');
    await deadlineInput.trigger('blur');

    expect(wrapper.emitted('save:soft_deadline')).toEqual([['2026-08-23']]);
  });

  it('開始予定日は日付文字列のまま送り、期限より後なら送らずに警告する', async () => {
    const wrapper = mount(TaskDetailHub, {
      props: {
        task: { ...task, soft_deadline: '2026-08-20T00:00:00.000Z', planned_start_on: null },
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
      },
    });

    await wrapper.get('button[aria-label="開始予定日を編集"]').trigger('click');
    const input = wrapper.get('input[aria-label="開始予定日"]');
    await input.setValue('2026-08-21');
    await input.trigger('blur');
    expect(wrapper.emitted('save:planned_start_on')).toBeUndefined();
    expect(wrapper.text()).toContain('開始予定日が期限より後になっています');

    // 期限ちょうどは通す
    await input.setValue('2026-08-20');
    await input.trigger('blur');
    expect(wrapper.emitted('save:planned_start_on')).toEqual([['2026-08-20']]);
  });

  function mountDeadlines(soft: string | null, hard: string | null) {
    return mount(TaskDetailHub, {
      props: {
        task: { ...task, soft_deadline: soft, hard_deadline: hard },
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
      },
    });
  }

  it('期限が最終期限と同じ日か後なら送らずに警告し、編集を開いたままにする', async () => {
    const wrapper = mountDeadlines(null, '2026-08-20T00:00:00.000Z');

    await wrapper.get('button[aria-label="ソフト期限を編集"]').trigger('click');
    const input = wrapper.get('input[aria-label="ソフト期限"]');
    for (const value of ['2026-08-20', '2026-08-21']) {
      await input.setValue(value);
      await input.trigger('blur');
      expect(wrapper.emitted('save:soft_deadline'), value).toBeUndefined();
      expect(wrapper.get('[role="alert"]').text()).toBe(DEADLINE_ORDER_MESSAGE);
      expect(wrapper.find('input[aria-label="ソフト期限"]').exists()).toBe(true);
    }

    // 前の日なら送る
    await input.setValue('2026-08-19');
    await input.trigger('blur');
    expect(wrapper.emitted('save:soft_deadline')).toEqual([['2026-08-19']]);
  });

  it('最終期限が期限と同じ日か前なら送らずに警告する', async () => {
    const wrapper = mountDeadlines('2026-08-20T00:00:00.000Z', null);

    await wrapper.get('button[aria-label="ハード期限を編集"]').trigger('click');
    const input = wrapper.get('input[aria-label="ハード期限"]');
    for (const value of ['2026-08-20', '2026-08-19']) {
      await input.setValue(value);
      await input.trigger('blur');
      expect(wrapper.emitted('save:hard_deadline'), value).toBeUndefined();
      expect(wrapper.text()).toContain(DEADLINE_ORDER_MESSAGE);
    }

    await input.setValue('2026-08-21');
    await input.trigger('blur');
    expect(wrapper.emitted('save:hard_deadline')).toEqual([['2026-08-21']]);
  });

  it('保存済みの最終期限が同じ日の 12 時なら、その日の期限は止めずに送る', async () => {
    const wrapper = mountDeadlines(null, '2026-08-20T12:00:00Z');

    await wrapper.get('button[aria-label="ソフト期限を編集"]').trigger('click');
    const input = wrapper.get('input[aria-label="ソフト期限"]');
    await input.setValue('2026-08-20');
    await input.trigger('blur');

    expect(wrapper.emitted('save:soft_deadline')).toEqual([['2026-08-20']]);
    expect(wrapper.text()).not.toContain(DEADLINE_ORDER_MESSAGE);
  });

  it('does not save an empty progress value as zero percent', async () => {
    const wrapper = mount(TaskDetailHub, {
      props: {
        task,
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
      },
    });

    const progressButton = wrapper.findAll('button').find((button) => button.text() === '30%');
    expect(progressButton).toBeDefined();
    await progressButton!.trigger('click');
    const progressInput = wrapper.get('input[aria-label="進捗率"]');
    await progressInput.setValue('');
    await progressInput.trigger('blur');

    expect(wrapper.emitted('save:progress_pct')).toBeUndefined();
    expect(wrapper.find('input[aria-label="進捗率"]').exists()).toBe(false);
    expect(wrapper.text()).toContain('30%');
  });

  it('emits delete-request when the kebab menu delete item is selected', async () => {
    const wrapper = mount(TaskDetailHub, {
      props: {
        task,
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
      },
      attachTo: document.body,
    });

    await wrapper.get('button[aria-label="タスク操作"]').trigger('click');
    const deleteItem = document.body.querySelector(
      '[role="menu"][aria-label="タスク操作"] [role="menuitem"]',
    );
    expect(deleteItem?.textContent).toContain('削除');
    deleteItem?.dispatchEvent(new MouseEvent('click', { bubbles: true }));

    expect(wrapper.emitted('delete-request')).toHaveLength(1);
    wrapper.unmount();
  });

  it('タスクのラベルをチップ表示する', () => {
    const wrapper = mount(TaskDetailHub, {
      props: {
        task: { ...task, labels: [bugLabel] },
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
      },
    });

    const section = wrapper.get('[data-task-labels]');
    expect(section.text()).toContain('bug');
    expect(section.text()).not.toContain('ラベルなし');
  });

  it('未付与ラベルのチェックで既存 ID に追加した save:label_ids を emit する', async () => {
    const wrapper = mount(TaskDetailHub, {
      props: {
        task: { ...task, labels: [bugLabel] },
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
        projectLabels: [bugLabel, featureLabel],
      },
      attachTo: document.body,
    });

    await wrapper.get('button[aria-label="ラベルを編集"]').trigger('click');
    const items = document.body.querySelectorAll('[role="menuitemcheckbox"]');
    const featureItem = Array.from(items).find((el) => el.textContent?.includes('feature'));
    expect(featureItem).toBeDefined();
    featureItem?.dispatchEvent(new MouseEvent('click', { bubbles: true }));

    expect(wrapper.emitted('save:label_ids')).toEqual([[['label-bug', 'label-feature']]]);
    wrapper.unmount();
  });

  it('ラベル一覧の取得失敗時は「ラベルがありません」ではなくエラーを表示する', async () => {
    const wrapper = mount(TaskDetailHub, {
      props: {
        task,
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
        projectLabelsError: true,
      },
      attachTo: document.body,
    });

    await wrapper.get('button[aria-label="ラベルを編集"]').trigger('click');
    expect(document.body.textContent).toContain('ラベルを読み込めませんでした');
    expect(document.body.textContent).not.toContain('ラベルがありません');
    wrapper.unmount();
  });

  it('取得失敗時にキャッシュ済みラベルが残っていてもチェックボックスを無効化する', async () => {
    const wrapper = mount(TaskDetailHub, {
      props: {
        task,
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
        projectLabels: [bugLabel],
        projectLabelsError: true,
      },
      attachTo: document.body,
    });

    await wrapper.get('button[aria-label="ラベルを編集"]').trigger('click');
    const items = document.body.querySelectorAll('[role="menuitemcheckbox"]');
    expect(items.length).toBe(1);
    expect(items[0].getAttribute('aria-disabled')).toBe('true');
    wrapper.unmount();
  });

  it('ラベル一覧の取得中は編集ボタンを無効化する', () => {
    const wrapper = mount(TaskDetailHub, {
      props: {
        task,
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
        projectLabelsLoading: true,
      },
    });

    expect(wrapper.get('button[aria-label="ラベルを編集"]').attributes('disabled')).toBeDefined();
  });

  it('付与済みラベルのチェック解除で除外した save:label_ids を emit する', async () => {
    const wrapper = mount(TaskDetailHub, {
      props: {
        task: { ...task, labels: [bugLabel] },
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
        projectLabels: [bugLabel, featureLabel],
      },
      attachTo: document.body,
    });

    await wrapper.get('button[aria-label="ラベルを編集"]').trigger('click');
    const items = document.body.querySelectorAll('[role="menuitemcheckbox"]');
    const bugItem = Array.from(items).find((el) => el.textContent?.includes('bug'));
    expect(bugItem).toBeDefined();
    bugItem?.dispatchEvent(new MouseEvent('click', { bubbles: true }));

    expect(wrapper.emitted('save:label_ids')).toEqual([[[]]]);
    wrapper.unmount();
  });

  it('プロジェクト一覧に無いラベルも送信集合に保持する（古い一覧キャッシュで暗黙解除しない）', async () => {
    const staleLabel: components['schemas']['LabelResponse'] = {
      ...bugLabel,
      id: 'label-stale',
      name: 'stale',
    };
    const wrapper = mount(TaskDetailHub, {
      props: {
        task: { ...task, labels: [bugLabel, staleLabel] },
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
        projectLabels: [bugLabel, featureLabel],
      },
      attachTo: document.body,
    });

    await wrapper.get('button[aria-label="ラベルを編集"]').trigger('click');
    const items = document.body.querySelectorAll('[role="menuitemcheckbox"]');
    const featureItem = Array.from(items).find((el) => el.textContent?.includes('feature'));
    expect(featureItem).toBeDefined();
    featureItem?.dispatchEvent(new MouseEvent('click', { bubbles: true }));

    // label-stale が一覧に無いのはキャッシュが古いだけかもしれないので落とさない
    expect(wrapper.emitted('save:label_ids')).toEqual([
      [['label-bug', 'label-stale', 'label-feature']],
    ]);
    wrapper.unmount();
  });
});

/*
 * 保存は emit した直後に編集器を閉じ、失敗は親が fieldErrors で返す。以前は失敗すると
 * 入力した下書きがどこにも残らず、書き直すしかなかった（TASK-250）。
 */
describe('TaskDetailHub の保存に失敗した下書き', () => {
  const FAILED = '更新に失敗しました';

  function mountHub() {
    return mount(TaskDetailHub, {
      props: { task, projectKey: 'TEST', statuses: [], statusId: task.status_id },
    });
  }

  function resumeButton(wrapper: ReturnType<typeof mountHub>) {
    return wrapper.findAll('button').find((button) => button.text() === '編集を再開');
  }

  async function resume(wrapper: ReturnType<typeof mountHub>) {
    const button = resumeButton(wrapper);
    expect(button, '「編集を再開」が無い').toBeDefined();
    await button!.trigger('click');
  }

  function titleInput(wrapper: ReturnType<typeof mountHub>) {
    return wrapper.get('input[aria-label="タイトル"]');
  }

  async function saveTitle(wrapper: ReturnType<typeof mountHub>, value: string) {
    await wrapper.get('h1').trigger('click');
    await titleInput(wrapper).setValue(value);
    await titleInput(wrapper).trigger('blur');
  }

  it('タイトルの保存に失敗したら、編集を再開すると入力した値が入っている', async () => {
    const wrapper = mountHub();
    await saveTitle(wrapper, '書きかけのタイトル');
    expect(wrapper.emitted('save:title')).toEqual([['書きかけのタイトル']]);
    expect(wrapper.find('input[aria-label="タイトル"]').exists()).toBe(false);

    await wrapper.setProps({ fieldErrors: { title: FAILED } });
    await resume(wrapper);

    expect((titleInput(wrapper).element as HTMLInputElement).value).toBe('書きかけのタイトル');
  });

  it('説明の保存に失敗したら、編集を再開すると入力した値が入っている', async () => {
    const wrapper = mountHub();
    await wrapper
      .findAll('button')
      .find((button) => button.text() === '説明を追加')!
      .trigger('click');
    const textarea = wrapper.get('textarea[aria-label="説明"]');
    await textarea.setValue('書きかけの説明');
    await wrapper
      .findAll('button')
      .find((button) => button.text() === '保存')!
      .trigger('click');
    expect(wrapper.emitted('save:description')).toEqual([['書きかけの説明']]);

    await wrapper.setProps({ fieldErrors: { description: FAILED } });
    await resume(wrapper);

    const reopened = wrapper.get('textarea[aria-label="説明"]');
    expect((reopened.element as HTMLTextAreaElement).value).toBe('書きかけの説明');
  });

  it('保存し直しも失敗したら、新しく入力した値から再開できる', async () => {
    const wrapper = mountHub();
    await saveTitle(wrapper, '一度目');
    await wrapper.setProps({ fieldErrors: { title: FAILED } });
    await resume(wrapper);
    await titleInput(wrapper).setValue('二度目');
    await titleInput(wrapper).trigger('blur');

    // 親は保存を始めると失敗表示を消し、また失敗すると立て直す
    await wrapper.setProps({ fieldErrors: {} });
    await wrapper.setProps({ fieldErrors: { title: FAILED } });
    await resume(wrapper);

    expect((titleInput(wrapper).element as HTMLInputElement).value).toBe('二度目');
  });

  it('保存し直して成功したら下書きは捨て、次に開くと task の値が入っている', async () => {
    const wrapper = mountHub();
    await saveTitle(wrapper, '一度目');
    await wrapper.setProps({ fieldErrors: { title: FAILED } });
    await resume(wrapper);
    await titleInput(wrapper).setValue('二度目');
    await titleInput(wrapper).trigger('blur');
    expect(wrapper.emitted('save:title')).toEqual([['一度目'], ['二度目']]);

    await wrapper.setProps({ fieldErrors: {}, task: { ...task, title: '二度目' } });
    expect(resumeButton(wrapper)).toBeUndefined();
    expect(wrapper.text()).not.toContain(FAILED);

    await wrapper.get('h1').trigger('click');
    expect((titleInput(wrapper).element as HTMLInputElement).value).toBe('二度目');
  });

  it('Esc で取り消したら下書きを残さない', async () => {
    const wrapper = mountHub();
    await wrapper.get('h1').trigger('click');
    await titleInput(wrapper).setValue('取り消す');
    await titleInput(wrapper).trigger('keydown', { key: 'Escape' });

    expect(wrapper.emitted('save:title')).toBeUndefined();
    expect(wrapper.find('input[aria-label="タイトル"]').exists()).toBe(false);
    expect(resumeButton(wrapper)).toBeUndefined();
    expect(wrapper.text()).not.toContain(FAILED);
  });

  it('失敗後に再開した編集を Esc で取り消したら、その下書きも捨てる', async () => {
    const wrapper = mountHub();
    await saveTitle(wrapper, '書きかけ');
    await wrapper.setProps({ fieldErrors: { title: FAILED } });
    await resume(wrapper);
    await titleInput(wrapper).trigger('keydown', { key: 'Escape' });

    expect(resumeButton(wrapper)).toBeUndefined();
    await wrapper.get('h1').trigger('click');
    expect((titleInput(wrapper).element as HTMLInputElement).value).toBe(task.title);
  });

  // 「クリア」は失う入力が無いので再開ボタンを出さない。前に成功した下書きで出してもいけない
  it('説明の保存に成功したあと「クリア」で失敗しても、前の値で編集を再開させない', async () => {
    const wrapper = mountHub();
    await wrapper
      .findAll('button')
      .find((button) => button.text() === '説明を追加')!
      .trigger('click');
    const textarea = wrapper.get('textarea[aria-label="説明"]');
    await textarea.setValue('成功した説明');
    await wrapper
      .findAll('button')
      .find((button) => button.text() === '保存')!
      .trigger('click');
    await wrapper.setProps({ fieldErrors: {}, task: { ...task, description: '成功した説明' } });

    await wrapper
      .findAll('button')
      .find((button) => button.text() === '成功した説明')!
      .trigger('click');
    await wrapper
      .findAll('button')
      .find((button) => button.text() === 'クリア')!
      .trigger('click');
    // クリアは確認を挟む
    (wrapper.findAllComponents(ConfirmDialog)[1]!.props('onConfirm') as () => void)();
    await nextTick();
    expect(wrapper.emitted('save:description')).toEqual([['成功した説明'], [null]]);

    await wrapper.setProps({ fieldErrors: { description: FAILED } });
    expect(wrapper.text()).toContain(FAILED);
    expect(resumeButton(wrapper)).toBeUndefined();
  });

  // 分割ビューのペインは別のタスクへ移っても同じインスタンスで、失敗表示も残りうる
  it('別のタスクへ移ったら、前のタスクの下書きを出さない', async () => {
    const wrapper = mountHub();
    await saveTitle(wrapper, '前のタスクの下書き');
    await wrapper.setProps({ fieldErrors: { title: FAILED } });

    await wrapper.setProps({ task: { ...task, id: 'other-task', title: '別のタスク' } });

    expect(resumeButton(wrapper)).toBeUndefined();
    await wrapper.get('h1').trigger('click');
    expect((titleInput(wrapper).element as HTMLInputElement).value).toBe('別のタスク');
  });
});

describe('TaskDetailHub description KFM 表示', () => {
  const RENDERED_HTML = '<h1 id="user-content-task-uuid-h">見出し</h1><p><strong>強調</strong></p>';

  it('descriptionHtml が最新 description の描画なら kfm-content の器へ v-html でそのまま入る', () => {
    const wrapper = mount(TaskDetailHub, {
      props: {
        task: { ...task, description: '# 見出し\n\n**強調**' },
        descriptionHtml: RENDERED_HTML,
        descriptionSource: '# 見出し\n\n**強調**',
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
      },
    });

    const container = wrapper.get('[data-task-description-html]');
    expect(container.classes()).toContain('kfm-content');
    expect(container.element.innerHTML).toBe(RENDERED_HTML);
    // 生テキスト表示 (プレーン分岐) は出ない
    expect(wrapper.find('p.whitespace-pre-wrap').exists()).toBe(false);
  });

  it('descriptionHtml が無ければ生テキストのプレーン表示のまま (クライアント再パースなし)', () => {
    const wrapper = mount(TaskDetailHub, {
      props: {
        task: { ...task, description: '**強調** <em>raw</em>' },
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
      },
    });

    // Markdown 記法が文字のまま見える = クライアントでは一切パースしていない
    expect(wrapper.get('p.whitespace-pre-wrap').text()).toContain('**強調**');
    expect(wrapper.find('[data-task-description-html]').exists()).toBe(false);
    expect(wrapper.find('strong').exists()).toBe(false);
    // 生テキスト中のタグ断片も要素化されない (v-html へ流れていない)
    expect(wrapper.find('em').exists()).toBe(false);
  });

  it('プレーン表示ではメンションの印を札と同じ字（@今の名）にし、id を出さない', () => {
    const alice = '11111111-1111-4111-8111-111111111111';
    const former = '22222222-2222-4222-8222-222222222222';
    const wrapper = mount(TaskDetailHub, {
      props: {
        task: {
          ...task,
          description: `見て <@${alice}> と <@${former}> へ`,
          mentions: {
            [alice]: { status: 'member', name: 'alice' },
            // 外れた者でも名前が引ければ名で出る
            [former]: { status: 'outside', name: 'former' },
          },
        },
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
      },
    });

    const text = wrapper.get('p.whitespace-pre-wrap').text();
    expect(text).toBe('見て @alice と @former へ');
    expect(text).not.toContain(alice);
    expect(text).not.toContain('<@');
  });

  it('既存の素の @username はそのまま描く（札にも別の字にもしない）', () => {
    const wrapper = mount(TaskDetailHub, {
      props: {
        task: {
          ...task,
          description: '@alice さん bob@example.com',
          mentions: {},
        },
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
      },
    });

    expect(wrapper.get('p.whitespace-pre-wrap').text()).toBe('@alice さん bob@example.com');
  });

  it('KFM 表示中は鉛筆ボタンから編集へ入り、textarea には生テキストが入る', async () => {
    const wrapper = mount(TaskDetailHub, {
      props: {
        task: { ...task, description: '# 見出し' },
        descriptionHtml: RENDERED_HTML,
        descriptionSource: '# 見出し',
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
      },
    });

    await wrapper.get('button[aria-label="説明を編集"]').trigger('click');
    const textarea = wrapper.get('textarea[aria-label="説明"]');
    expect((textarea.element as HTMLTextAreaElement).value).toBe('# 見出し');
    // 編集中は KFM 表示は消える
    expect(wrapper.find('[data-task-description-html]').exists()).toBe(false);
  });

  it('説明が空なら descriptionHtml が残っていても stale HTML を出さない', () => {
    const wrapper = mount(TaskDetailHub, {
      props: {
        task: { ...task, description: null },
        descriptionHtml: RENDERED_HTML,
        descriptionSource: '# 見出し',
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
      },
    });

    expect(wrapper.find('[data-task-description-html]').exists()).toBe(false);
    expect(wrapper.text()).toContain('説明を追加');
  });

  it('descriptionSource が最新 description と不一致なら stale HTML を捨ててプレーン表示へ倒す', () => {
    // 保存直後 (reload 完了前)・reload 失敗・他者更新の再現: クライアントの
    // task.description は新しいが、サーバ描画 HTML は古い本文のもの。
    const wrapper = mount(TaskDetailHub, {
      props: {
        task: { ...task, description: '新しい本文' },
        descriptionHtml: RENDERED_HTML,
        descriptionSource: '# 見出し\n\n**強調**',
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
      },
    });

    expect(wrapper.find('[data-task-description-html]').exists()).toBe(false);
    expect(wrapper.get('p.whitespace-pre-wrap').text()).toContain('新しい本文');
    // 古い HTML の中身がどこにも出ていない
    expect(wrapper.find('strong').exists()).toBe(false);
  });

  it('descriptionSource 無しで descriptionHtml だけ渡されたら v-html しない (対渡しの強制)', () => {
    const wrapper = mount(TaskDetailHub, {
      props: {
        task: { ...task, description: '# 見出し\n\n**強調**' },
        descriptionHtml: RENDERED_HTML,
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
      },
    });

    expect(wrapper.find('[data-task-description-html]').exists()).toBe(false);
    expect(wrapper.get('p.whitespace-pre-wrap').text()).toContain('**強調**');
  });
});

describe('TaskDetailHub v-html 経路の source 契約', () => {
  it('SFC 内の v-html は照合済み freshDescriptionHtml へのバインド 1 箇所だけ', () => {
    const source = readFileSync(path.join(__dirname, '../TaskDetailHub.vue'), 'utf-8');
    const bindings = source.match(/v-html="[^"]*"/g) ?? [];
    expect(bindings).toEqual(['v-html="freshDescriptionHtml"']);
  });
});

describe('TaskDetailHub ステータスの操作', () => {
  function mountWithStatuses(statusId: string) {
    return mount(TaskDetailHub, {
      props: { task, projectKey: 'TEST', statuses, statusId },
    });
  }

  it('右矢印はワークフロー順の次のステータスへ進める', async () => {
    const wrapper = mountWithStatuses('status-id');

    await wrapper.get('button[aria-label="In Progress にする"]').trigger('click');

    expect(wrapper.emitted('update:statusId')).toEqual([['status-progress']]);
  });

  it('最後のステータスでは右矢印を出さない', () => {
    const wrapper = mountWithStatuses('status-done');

    expect(
      wrapper.findAll('button').some((b) => b.attributes('aria-label')?.endsWith('にする')),
    ).toBe(false);
  });

  it('チェックは完了扱いのステータスへ移す', async () => {
    const wrapper = mountWithStatuses('status-id');

    await wrapper.get('button[aria-label="Done にする（完了）"]').trigger('click');

    expect(wrapper.emitted('update:statusId')).toEqual([['status-done']]);
  });
});

// 担当者全員の完了が必要なタスクは、未完了の担当者が残る間は完了へ直接移せない（backend が 409 で止める）
describe('TaskDetailHub の完了条件', () => {
  const alpha = { id: 'user-alpha', username: 'alpha', avatar_url: null };
  const beta = { id: 'user-beta', username: 'beta', avatar_url: null };

  function mountRule(
    taskPatch: Partial<TaskDetail>,
    extra: Record<string, unknown> = {},
    statusId = 'status-id',
  ) {
    return mount(TaskDetailHub, {
      props: {
        task: { ...task, ...taskPatch },
        projectKey: 'TEST',
        statuses,
        statusId,
        currentUserId: 'user-alpha',
        ...extra,
      },
    });
  }

  const pending = {
    require_all_assignees_done: true,
    assignees: [
      { role: 'assignee', user: alpha, completed_at: null },
      { role: 'assignee', user: beta, completed_at: '2026-07-16T01:00:00Z' },
    ],
  };

  function doneItem(wrapper: ReturnType<typeof mountRule>) {
    return wrapper
      .get('[role="menu"][aria-label="ステータス"]')
      .findAll('[role="menuitemradio"]')
      .find((item) => item.text().includes('Done'))!;
  }

  it('未完了の担当者が残るなら、✓ ボタンを出さず、メニューの完了を無効にする', () => {
    const wrapper = mountRule(pending);

    expect(wrapper.find('button[aria-label="Done にする（完了）"]').exists()).toBe(false);
    expect(doneItem(wrapper).attributes('aria-disabled')).toBe('true');
    // 完了でない項目は選べる
    const todo = wrapper
      .get('[role="menu"][aria-label="ステータス"]')
      .findAll('[role="menuitemradio"]')
      .find((item) => item.text().includes('In Progress'))!;
    expect(todo.attributes('aria-disabled')).not.toBe('true');
  });

  it('「次へ」が完了ステータスなら無効にする', () => {
    const wrapper = mountRule(pending, {}, 'status-progress');

    expect(wrapper.get('button[aria-label="Done にする"]').attributes('disabled')).toBeDefined();
  });

  it('設定が off なら従来どおり ✓ ボタンも完了の項目も使える', async () => {
    const wrapper = mountRule({ ...pending, require_all_assignees_done: false });

    expect(doneItem(wrapper).attributes('aria-disabled')).not.toBe('true');
    await wrapper.get('button[aria-label="Done にする（完了）"]').trigger('click');
    expect(wrapper.emitted('update:statusId')).toEqual([['status-done']]);
  });

  it('全員が完了にしていれば（未完了が残らなければ）✓ ボタンを出す', () => {
    const wrapper = mountRule({
      require_all_assignees_done: true,
      assignees: [{ role: 'assignee', user: beta, completed_at: '2026-07-16T01:00:00Z' }],
    });

    expect(wrapper.find('button[aria-label="Done にする（完了）"]').exists()).toBe(true);
  });

  it('完了条件の操作を emit する', async () => {
    const wrapper = mountRule(pending);

    await wrapper.get('input[name="require-all-assignees-done"]').setValue(false);
    expect(wrapper.emitted('update:require-all')).toEqual([[false]]);

    const complete = wrapper.findAll('button').find((b) => b.text() === '自分の分を完了にする')!;
    await complete.trigger('click');
    expect(wrapper.emitted('complete:mine')).toHaveLength(1);
  });
});

// 担当者は一覧の行と同じ口を使うが、飛行中と失敗をこの画面へ渡さないと
// 「失敗しても無表示」「送信中も押せて 2 回目が無言で捨てられる」状態になる
describe('TaskDetailHub の担当者', () => {
  const members = [{ id: 'user-1', username: 'yupix', avatar_url: null }];

  function mountWithAssignee(extra: Record<string, unknown>) {
    return mount(TaskDetailHub, {
      props: {
        task,
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
        members,
        ...extra,
      },
    });
  }

  it('更新中はピッカーを押せない', () => {
    const wrapper = mountWithAssignee({ assigneeUpdating: true });

    const picker = wrapper.findComponent({ name: 'TaskAssigneePicker' });
    expect(picker.exists()).toBe(true);
    expect(picker.get('button').attributes('disabled')).toBeDefined();
  });

  it('ピッカーの一覧は本文のスクロール領域で切れないようトップレイヤーに出す', () => {
    const wrapper = mountWithAssignee({});

    const picker = wrapper.findComponent({ name: 'TaskAssigneePicker' });
    expect(picker.get('button').element.closest('[data-menu-top-layer]')).not.toBeNull();
  });

  it('更新の失敗をその場に出す', () => {
    const wrapper = mountWithAssignee({ assigneeError: '更新に失敗しました' });

    expect(wrapper.text()).toContain('更新に失敗しました');
  });

  it('通常は押せる', () => {
    const wrapper = mountWithAssignee({});

    const picker = wrapper.findComponent({ name: 'TaskAssigneePicker' });
    expect(picker.get('button').attributes('disabled')).toBeUndefined();
  });
});

describe('TaskDetailHub の期限の表示', () => {
  const pastDeadlines = {
    ...task,
    soft_deadline: '2026-07-10T00:00:00',
    hard_deadline: '2026-07-11T00:00:00',
  };

  function mountWithStatus(statusId: string, completedAt: string | null = null) {
    return mount(TaskDetailHub, {
      props: {
        task: { ...pastDeadlines, completed_at: completedAt },
        projectKey: 'TEST',
        statuses,
        statusId,
      },
    });
  }

  function buttonByText(wrapper: ReturnType<typeof mountWithStatus>, label: string) {
    const button = wrapper.findAll('button').find((b) => b.text() === label);
    expect(button, `期限の button「${label}」が無い`).toBeDefined();
    return button!;
  }

  afterEach(() => {
    vi.useRealTimers();
  });

  it('完了の状態では、過ぎた期限（ソフトとハード）を赤くせず、日付だけを出す', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-07-14T12:00:00'));
    const wrapper = mountWithStatus('status-done');
    expect(buttonByText(wrapper, '7月10日').classes()).not.toContain('text-destructive');
    expect(buttonByText(wrapper, '7月11日').classes()).not.toContain('text-destructive');
    expect(wrapper.text()).not.toContain('超過');
  });

  it('期限より後の日に完了した物は、ソフトとハードの両方に遅れた日数を中立で添える', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-07-20T12:00:00'));
    const wrapper = mountWithStatus('status-done', '2026-07-14T09:00:00');
    expect(buttonByText(wrapper, '7月10日（4日超過）').classes()).not.toContain('text-destructive');
    expect(buttonByText(wrapper, '7月11日（3日超過）').classes()).not.toContain('text-destructive');
  });

  it('期限の前に完了した物は、ソフトとハードの両方で日付だけを出す', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-07-20T12:00:00'));
    const wrapper = mountWithStatus('status-done', '2026-07-06T09:00:00');
    buttonByText(wrapper, '7月10日');
    buttonByText(wrapper, '7月11日');
    expect(wrapper.text()).not.toContain('超過');
  });

  it('完了していない状態では、今までどおり過ぎた期限を赤く超過と出す', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-07-14T12:00:00'));
    const wrapper = mountWithStatus('status-progress');
    expect(buttonByText(wrapper, '4日超過').classes()).toContain('text-destructive');
    expect(buttonByText(wrapper, '3日超過').classes()).toContain('text-destructive');
  });
});

describe('TaskDetailHub 説明の編集', () => {
  async function startEdit(description: string | null = '元の説明') {
    const wrapper = mount(TaskDetailHub, {
      props: {
        task: { ...task, description },
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
      },
    });
    const opener = wrapper
      .findAll('button')
      .find((b) => b.text() === (description ?? '説明を追加'));
    await opener!.trigger('click');
    return wrapper;
  }
  const dialogs = (w: Awaited<ReturnType<typeof startEdit>>) => w.findAllComponents(ConfirmDialog);
  const button = (w: Awaited<ReturnType<typeof startEdit>>, label: string) =>
    w.findAll('button').find((b) => b.text() === label);

  it('保存とキャンセルのボタンを出す', async () => {
    const wrapper = await startEdit();
    expect(button(wrapper, '保存')).toBeTruthy();
    expect(button(wrapper, 'キャンセル')).toBeTruthy();
  });

  it('blur では保存せず編集を続ける', async () => {
    const wrapper = await startEdit();
    const textarea = wrapper.get('textarea[aria-label="説明"]');
    await textarea.setValue('書きかけ');
    await textarea.trigger('blur');
    expect(wrapper.emitted('save:description')).toBeUndefined();
    expect(wrapper.find('textarea[aria-label="説明"]').exists()).toBe(true);
  });

  it('保存ボタンで保存して閉じる', async () => {
    const wrapper = await startEdit();
    await wrapper.get('textarea[aria-label="説明"]').setValue('新しい説明');
    await button(wrapper, '保存')!.trigger('click');
    expect(wrapper.emitted('save:description')).toEqual([['新しい説明']]);
    expect(wrapper.find('textarea[aria-label="説明"]').exists()).toBe(false);
  });

  it('編集中に別のタスクへ替わったら編集を閉じ、前の下書きを移った先へ保存しない', async () => {
    const wrapper = await startEdit();
    await wrapper.get('textarea[aria-label="説明"]').setValue('A の書きかけ');
    await wrapper.setProps({ task: { ...task, id: 'task-b', description: 'B の説明' } });
    expect(wrapper.find('textarea[aria-label="説明"]').exists()).toBe(false);
    expect(wrapper.emitted('save:description')).toBeUndefined();
    expect(wrapper.text()).toContain('B の説明');
  });

  it('変更ありの Esc は確認を出し、取りやめると編集が続く', async () => {
    const wrapper = await startEdit();
    const textarea = wrapper.get('textarea[aria-label="説明"]');
    await textarea.setValue('書きかけ');
    await textarea.trigger('keydown', { key: 'Escape' });
    const confirm = dialogs(wrapper)[0]!;
    expect(confirm.props('open')).toBe(true);
    expect(wrapper.find('textarea[aria-label="説明"]').exists()).toBe(true);
    (confirm.props('onCancel') as () => void)();
    await nextTick();
    expect(dialogs(wrapper)[0]!.props('open')).toBe(false);
    expect((wrapper.get('textarea[aria-label="説明"]').element as HTMLTextAreaElement).value).toBe(
      '書きかけ',
    );
  });

  it('確認で破棄すると元に戻り、保存はしない', async () => {
    const wrapper = await startEdit();
    await wrapper.get('textarea[aria-label="説明"]').setValue('書きかけ');
    await button(wrapper, 'キャンセル')!.trigger('click');
    (dialogs(wrapper)[0]!.props('onConfirm') as () => void)();
    await nextTick();
    expect(wrapper.find('textarea[aria-label="説明"]').exists()).toBe(false);
    expect(wrapper.emitted('save:description')).toBeUndefined();
    expect(wrapper.text()).toContain('元の説明');
  });

  it('変更ありのままタイトルの編集を始めると確認が出て、取りやめると説明の編集と内容が残る', async () => {
    const wrapper = await startEdit();
    await wrapper.get('textarea[aria-label="説明"]').setValue('書きかけ');
    await wrapper
      .findAll('button')
      .find((b) => b.text() === task.title)!
      .trigger('click');
    const confirm = dialogs(wrapper)[0]!;
    expect(confirm.props('open')).toBe(true);
    (confirm.props('onCancel') as () => void)();
    await nextTick();
    expect((wrapper.get('textarea[aria-label="説明"]').element as HTMLTextAreaElement).value).toBe(
      '書きかけ',
    );
    expect(wrapper.emitted('save:title')).toBeUndefined();
  });

  it('変更が無ければ Esc もキャンセルも確認なしで閉じる', async () => {
    const wrapper = await startEdit();
    await wrapper.get('textarea[aria-label="説明"]').trigger('keydown', { key: 'Escape' });
    expect(dialogs(wrapper)[0]!.props('open')).toBe(false);
    expect(wrapper.find('textarea[aria-label="説明"]').exists()).toBe(false);
  });

  it('クリアは確認後にだけ空にして保存する', async () => {
    const wrapper = await startEdit();
    await button(wrapper, 'クリア')!.trigger('click');
    expect(wrapper.emitted('save:description')).toBeUndefined();
    const confirm = dialogs(wrapper)[1]!;
    expect(confirm.props('open')).toBe(true);
    (confirm.props('onConfirm') as () => void)();
    await nextTick();
    expect(wrapper.emitted('save:description')).toEqual([[null]]);
  });
});

describe('TaskDetailHub 読み込みの失敗', () => {
  it('失敗を出し、再試行で表示中の失敗した query を取り直す', async () => {
    let calls = 0;
    const Host = defineComponent({
      components: { TaskDetailHub },
      setup() {
        const q = useQuery({
          queryKey: ['hub-retry'],
          queryFn: () => {
            calls += 1;
            return Promise.reject(new Error('x'));
          },
          retry: false,
        });
        return { q };
      },
      template: '<TaskDetailHub :error="q.isError.value" />',
    });
    const wrapper = mount(Host, {
      global: { plugins: [[VueQueryPlugin, { queryClient: new QueryClient() }]] },
    });
    await flushPromises();
    expect(calls).toBe(1);
    expect(wrapper.get('[data-testid="load-error"]').text()).toContain('読み込みに失敗しました');

    await wrapper.get('[data-testid="load-error"] button').trigger('click');
    await flushPromises();
    expect(calls).toBe(2);
  });
});

describe('TaskDetailHub のチェックリスト達成率', () => {
  const CHECKLIST = '- [x] a\n- [x] b\n- [ ] c\n- [ ] d\n- [ ] e';

  function mountWith(description: string | null, extra: Record<string, unknown> = {}) {
    return mount(TaskDetailHub, {
      props: {
        task: { ...task, description },
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
        ...extra,
      },
    });
  }

  it('説明の見出しの隣に完了数 / 総数を出す', () => {
    const wrapper = mountWith(CHECKLIST);
    const badge = wrapper.get('[data-checklist-progress]');

    expect(badge.text()).toContain('2 / 5');
    expect(badge.element.previousElementSibling?.textContent).toBe('説明');
  });

  it('KFM 描画の表示でも同じ値を出す', () => {
    const wrapper = mountWith(CHECKLIST, {
      descriptionHtml: '<ul><li>a</li></ul>',
      descriptionSource: CHECKLIST,
    });

    expect(wrapper.get('[data-checklist-progress]').text()).toContain('2 / 5');
  });

  it('タスクリストの無い説明・説明が null では出さない', () => {
    expect(mountWith('普通の説明').find('[data-checklist-progress]').exists()).toBe(false);
    expect(mountWith(null).find('[data-checklist-progress]').exists()).toBe(false);
  });

  it('progress_pct の「進捗」行は別の値のまま', () => {
    const wrapper = mount(TaskDetailHub, {
      props: {
        task: { ...task, description: CHECKLIST, progress_pct: 30 },
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
      },
    });

    expect(wrapper.get('[data-checklist-progress]').text()).toContain('2 / 5');
    expect(wrapper.text()).toContain('30%');
  });
});

describe('TaskDetailHub の説明のチェックボックスの切り替え', () => {
  const SOURCE = '- [x] 済み\n- [ ] 未了\n- [ ] 最後';
  const TOGGLED = '- [x] 済み\n- [x] 未了\n- [ ] 最後';
  let html = '';

  beforeAll(async () => {
    html = await renderDescription(SOURCE);
  });

  function mountWith(extra: Record<string, unknown> = {}) {
    const { attachTo, ...props } = extra;
    return mount(TaskDetailHub, {
      attachTo: attachTo as HTMLElement | undefined,
      props: {
        task: { ...task, description: SOURCE },
        descriptionHtml: html,
        descriptionSource: SOURCE,
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
        ...props,
      },
    });
  }

  const FAILED = '更新に失敗しました';

  const resumeButton = (wrapper: ReturnType<typeof mountWith>) =>
    wrapper.findAll('button').find((button) => button.text() === '編集を再開');

  /** 説明を編集して保存する（保存の成否は呼び出し側が props で表す） */
  async function editAndSave(wrapper: ReturnType<typeof mountWith>, value: string) {
    await wrapper.get('button[aria-label="説明を編集"]').trigger('click');
    await wrapper.get('textarea[aria-label="説明"]').setValue(value);
    await wrapper
      .findAll('button')
      .find((button) => button.text() === '保存')!
      .trigger('click');
  }

  const boxes = (wrapper: ReturnType<typeof mountWith>) =>
    wrapper.findAll<HTMLInputElement>('[data-task-description-html] input[type="checkbox"]');

  it('マウント後、位置の属性付きのチェックボックスは有効で、項目の文字が名前になる', async () => {
    const wrapper = mountWith();
    await nextTick();

    expect(boxes(wrapper).map((b) => b.element.disabled)).toEqual([false, false, false]);
    expect(boxes(wrapper).map((b) => b.attributes('aria-label'))).toEqual(['済み', '未了', '最後']);
  });

  it('貼り付けた画像の項目は、画像の名前を 1 回だけ名前にする（退避用の hidden のリンクを読まない）', async () => {
    const source =
      '- [ ] ![shot.png](/v1/drive/files/0f8fad5b-d9cb-469f-a165-70867728950e/content)';
    const wrapper = mountWith({
      task: { ...task, description: source },
      descriptionHtml: await renderDescription(source),
      descriptionSource: source,
    });
    await nextTick();

    expect(wrapper.find('[data-task-description-html] kfm-media a[hidden]').exists()).toBe(true);
    expect(boxes(wrapper).map((b) => b.attributes('aria-label'))).toEqual(['shot.png']);
  });

  it('説明の入力欄が貼り付けたファイルを上げている間は保存を止め、終われば保存できる', async () => {
    const wrapper = mountWith({ tenantId: 'tenant-1' });
    await wrapper.get('button[aria-label="説明を編集"]').trigger('click');
    await wrapper.get('textarea[aria-label="説明"]').setValue('画像つきの説明');
    const editor = wrapper.getComponent({ name: 'MarkdownEditorStub' });
    expect(typeof editor.props('uploadFile')).toBe('function');
    expect(editor.props('uploadScope')).toBe(task.id);
    const save = wrapper.findAll('button').find((button) => button.text() === '保存')!;

    editor.vm.$emit('update:uploading', true);
    await nextTick();
    expect(save.attributes('disabled')).toBeDefined();
    // ボタンを押さない経路（Mod-Enter）でも保存しない
    await save.trigger('click');
    editor.vm.$emit('submit');
    await nextTick();
    expect(wrapper.emitted('save:description')).toBeUndefined();

    editor.vm.$emit('update:uploading', false);
    await nextTick();
    expect(save.attributes('disabled')).toBeUndefined();
    editor.vm.$emit('submit');
    await nextTick();
    expect(wrapper.emitted('save:description')).toEqual([['画像つきの説明']]);
  });

  it('位置の属性が無いチェックボックスは disabled のまま', async () => {
    const plain = '<ul><li><input type="checkbox" disabled=""> 属性なし</li></ul>';
    const wrapper = mountWith({ descriptionHtml: plain });
    await nextTick();

    expect(boxes(wrapper)[0].element.disabled).toBe(true);
  });

  it('押すと save:description が、その項目だけ替えた本文で 1 回 emit される', async () => {
    const wrapper = mountWith();
    await nextTick();

    await boxes(wrapper)[1].setValue(true);

    expect(wrapper.emitted('save:description')).toEqual([[TOGGLED, { preserveWhitespace: true }]]);
  });

  it('チェックを外す操作は [x] を空白へ戻した本文を emit する', async () => {
    const wrapper = mountWith();
    await nextTick();

    await boxes(wrapper)[0].setValue(false);

    expect(wrapper.emitted('save:description')).toEqual([
      ['- [ ] 済み\n- [ ] 未了\n- [ ] 最後', { preserveWhitespace: true }],
    ]);
  });

  it('押した後に task.description が新しい本文になっても (HTML は古いまま) プレーン表示へ落ちない', async () => {
    const wrapper = mountWith();
    await nextTick();
    await boxes(wrapper)[1].setValue(true);

    await wrapper.setProps({ task: { ...task, description: TOGGLED } });

    expect(wrapper.find('[data-task-description-html]').exists()).toBe(true);
    expect(wrapper.find('p.whitespace-pre-wrap').exists()).toBe(false);
    expect(boxes(wrapper).map((b) => b.element.checked)).toEqual([true, true, false]);
    // 見出しの達成率は新しい本文に追従する
    expect(wrapper.get('[data-checklist-progress]').text()).toContain('2 / 3');
  });

  it('サーバが描いた新しい HTML が届いても DOM は同じ (差し替わらない)', async () => {
    const wrapper = mountWith();
    await nextTick();
    await boxes(wrapper)[1].setValue(true);
    await wrapper.setProps({ task: { ...task, description: TOGGLED } });
    const before = wrapper.get('[data-task-description-html]').element.innerHTML;

    await wrapper.setProps({
      descriptionHtml: await renderDescription(TOGGLED),
      descriptionSource: TOGGLED,
    });

    expect(wrapper.get('[data-task-description-html]').element.innerHTML).toBe(before);
  });

  it('保存中 (fieldUpdating.description) は全部 aria-disabled で、押しても emit せず戻す。終われば元に戻る', async () => {
    const wrapper = mountWith();
    await nextTick();
    await wrapper.setProps({ fieldUpdating: { description: true } });
    // disabled にするとフォーカスを持てないので aria-disabled にする
    expect(boxes(wrapper).map((b) => b.attributes('aria-disabled'))).toEqual([
      'true',
      'true',
      'true',
    ]);
    expect(boxes(wrapper).map((b) => b.element.disabled)).toEqual([false, false, false]);

    await boxes(wrapper)[1].setValue(true);
    expect(wrapper.emitted('save:description')).toBeUndefined();
    expect(boxes(wrapper)[1].element.checked).toBe(false);

    await wrapper.setProps({ fieldUpdating: { description: false } });
    expect(boxes(wrapper).map((b) => b.attributes('aria-disabled'))).toEqual([
      undefined,
      undefined,
      undefined,
    ]);
  });

  it('文字の無い項目 (画像だけ) は alt を名前にし、alt も無ければ「チェック項目 N」', async () => {
    const source = '- [ ] ![設計図](a.png)\n- [ ] ![](b.png)\n- [ ] 親\n  - [ ] 子';
    const wrapper = mountWith({
      task: { ...task, description: source },
      descriptionHtml: await renderDescription(source),
      descriptionSource: source,
    });
    await nextTick();

    // 入れ子の子の字は親の名前に混ぜない
    expect(boxes(wrapper).map((b) => b.attributes('aria-label'))).toEqual([
      '設計図',
      'チェック項目 2',
      '親',
      '子',
    ]);
  });

  /** 実際の親と同じく、emit の中で（同期に）楽観更新して保存中にする */
  function mountSavingOnToggle() {
    const holder: { wrapper?: ReturnType<typeof mountWith> } = {};
    const wrapper = mountWith({
      attachTo: document.body,
      'onSave:description': () => {
        void holder.wrapper!.setProps({
          task: { ...task, description: TOGGLED },
          fieldUpdating: { description: true },
        });
      },
    });
    holder.wrapper = wrapper;
    return wrapper;
  }

  it('保存中は押した項目のフォーカスを保ち、HTML が作り直されても同じ項目へ戻す', async () => {
    const wrapper = mountSavingOnToggle();
    await nextTick();
    boxes(wrapper)[1].element.focus();
    await boxes(wrapper)[1].setValue(true);
    await nextTick();

    // 作り直された input は保存中も aria-disabled なのでフォーカスを持てる
    expect(document.activeElement).toBe(boxes(wrapper)[1].element);
    expect(boxes(wrapper)[1].attributes('aria-disabled')).toBe('true');

    await wrapper.setProps({ fieldUpdating: { description: false } });
    expect(document.activeElement).toBe(boxes(wrapper)[1].element);
  });

  it('失敗で本文が戻って HTML が作り直されても、フォーカスは同じ項目に戻る', async () => {
    const wrapper = mountSavingOnToggle();
    await nextTick();
    boxes(wrapper)[1].element.focus();
    await boxes(wrapper)[1].setValue(true);
    await nextTick();

    await wrapper.setProps({
      task: { ...task, description: SOURCE },
      fieldUpdating: { description: false },
      fieldErrors: { description: FAILED },
    });

    await nextTick(); // フォーカスは描画の直後に戻す
    expect(document.activeElement).toBe(boxes(wrapper)[1].element);
    expect(boxes(wrapper)[1].element.checked).toBe(false);
  });

  it('保存中に別の項目へフォーカスを移していたら、失敗で HTML が作り直されてもそちらに残る', async () => {
    const wrapper = mountSavingOnToggle();
    await nextTick();
    boxes(wrapper)[1].element.focus();
    await boxes(wrapper)[1].setValue(true);
    await nextTick();
    boxes(wrapper)[2].element.focus();

    await wrapper.setProps({
      task: { ...task, description: SOURCE },
      fieldUpdating: { description: false },
      fieldErrors: { description: FAILED },
    });

    await nextTick(); // フォーカスは描画の直後に戻す
    expect(document.activeElement).toBe(boxes(wrapper)[2].element);
  });

  it('フォーカスが説明の外へ移っていたら、HTML が作り直されても奪わない', async () => {
    const wrapper = mountSavingOnToggle();
    await nextTick();
    boxes(wrapper)[1].element.focus();
    await boxes(wrapper)[1].setValue(true);
    await nextTick();
    (document.activeElement as HTMLElement).blur();

    await wrapper.setProps({
      task: { ...task, description: SOURCE },
      fieldUpdating: { description: false },
      fieldErrors: { description: FAILED },
    });
    await nextTick(); // フォーカスは描画の直後に戻す

    expect(document.activeElement).toBe(document.body);
  });

  it('タスクが替わったときの差し替えでは、前のタスクの項目へフォーカスを戻さない', async () => {
    const wrapper = mountWith({ attachTo: document.body });
    await nextTick();
    boxes(wrapper)[1].element.focus();

    await wrapper.setProps({
      task: { ...task, id: 'other-task', description: '- [ ] 別の\n- [ ] タスク\n- [ ] です' },
      descriptionHtml: undefined,
      descriptionSource: undefined,
    });
    await nextTick();

    expect(document.activeElement).not.toBeInstanceOf(HTMLInputElement);
  });

  it('失敗の戻し: description を元へ戻すとチェックも戻り、「更新に失敗しました」が出て「編集を再開」は出ない', async () => {
    const wrapper = mountWith();
    await nextTick();
    await boxes(wrapper)[1].setValue(true);
    await wrapper.setProps({
      task: { ...task, description: TOGGLED },
      fieldUpdating: { description: true },
    });

    await wrapper.setProps({
      task: { ...task, description: SOURCE },
      fieldUpdating: { description: false },
      fieldErrors: { description: '更新に失敗しました' },
    });

    expect(boxes(wrapper).map((b) => b.element.checked)).toEqual([true, false, false]);
    expect(wrapper.text()).toContain('更新に失敗しました');
    expect(wrapper.text()).not.toContain('編集を再開');
    expect(wrapper.get('[data-checklist-progress]').text()).toContain('1 / 3');
  });

  it('編集の保存に失敗して下書きを取り戻せる間は、チェックボックスを押せず、「編集を再開」が残る', async () => {
    const wrapper = mountWith();
    await nextTick();
    await editAndSave(wrapper, '書きかけの説明');
    await wrapper.setProps({ fieldErrors: { description: FAILED } });
    expect(resumeButton(wrapper)).toBeDefined();
    // 切り替えの保存が始まると失敗表示が消えて下書きが戻せなくなるので、押せない
    expect(boxes(wrapper).map((b) => b.attributes('aria-disabled'))).toEqual([
      'true',
      'true',
      'true',
    ]);

    await boxes(wrapper)[1].setValue(true);

    expect(wrapper.emitted('save:description')).toHaveLength(1); // 編集の保存だけ
    expect(boxes(wrapper)[1].element.checked).toBe(false);
    expect(resumeButton(wrapper)).toBeDefined();
    await resumeButton(wrapper)!.trigger('click');
    expect((wrapper.get('textarea[aria-label="説明"]').element as HTMLTextAreaElement).value).toBe(
      '書きかけの説明',
    );
  });

  it('対照: 前に成功した編集の下書きは、切り替えの失敗で「編集を再開」に出ない', async () => {
    const wrapper = mountWith();
    await nextTick();
    // 編集の保存は成功した（失敗表示は出ない。親が本文を保つ代わりに task はそのまま）
    await editAndSave(wrapper, '成功した説明');
    expect(resumeButton(wrapper)).toBeUndefined();

    await boxes(wrapper)[1].setValue(true);
    await wrapper.setProps({ fieldErrors: { description: FAILED } });

    expect(resumeButton(wrapper)).toBeUndefined();
  });

  it('対照: descriptionSource が description と違う (他の人が更新した) と HTML を出さず、押せない', async () => {
    const wrapper = mountWith({ descriptionSource: '- [ ] 古い本文' });
    await nextTick();

    expect(wrapper.find('[data-task-description-html]').exists()).toBe(false);
    expect(wrapper.find('[data-task-description-html] input').exists()).toBe(false);
    expect(wrapper.emitted('save:description')).toBeUndefined();
  });

  it('別のタスクへ移ったら、前のタスクの橋渡しの HTML を出さない', async () => {
    const wrapper = mountWith();
    await nextTick();
    await boxes(wrapper)[1].setValue(true);
    await wrapper.setProps({ task: { ...task, description: TOGGLED } });

    // 別のタスクが同じ本文を持っていても、HTML はまだ届いていない
    await wrapper.setProps({
      task: { ...task, id: 'other-task', description: TOGGLED },
      descriptionHtml: undefined,
      descriptionSource: undefined,
    });

    expect(wrapper.find('[data-task-description-html]').exists()).toBe(false);
  });

  it('編集中は KFM 表示 (チェックボックス) を出さない', async () => {
    const wrapper = mountWith();
    await nextTick();

    await wrapper.get('button[aria-label="説明を編集"]').trigger('click');
    await nextTick();

    expect(wrapper.find('[data-task-description-html]').exists()).toBe(false);
    expect(wrapper.find('[data-task-description-html] input').exists()).toBe(false);
  });
});

describe('TaskDetailHub のタスク参照カード', () => {
  afterEach(() => vi.unstubAllGlobals());
  it.each([true, false])('説明のタスクリンクの有無 %s に応じてカードを置く', async (linked) => {
    globalThis.ResizeObserver ??= class {
      observe() {}
      unobserve() {}
      disconnect() {}
    };
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    const wrapper = mount(TaskDetailHub, {
      props: {
        task: { ...task, description: 'TASK-58' },
        descriptionSource: 'TASK-58',
        descriptionHtml: linked
          ? '<a class="kfm-task-ref" href="/acme/projects/TASK/tasks/TASK-58">TASK-58</a>'
          : '<p>TASK-58</p>',
        projectKey: 'TASK',
        statuses: [],
        statusId: task.status_id,
        tenantId: 'tenant',
        mentionTenant: 'acme',
      },
    });
    await vi.waitFor(() => expect(wrapper.findAll('[role="dialog"]')).toHaveLength(linked ? 1 : 0));
    expect(fetch).not.toHaveBeenCalled();
  });
});
