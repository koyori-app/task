import { afterEach, describe, expect, it } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';

import TaskActivityEntry from '@/components/tasks/TaskActivityEntry.vue';
import type { ActivityLookups, HistoryEntry } from '@/lib/activity-timeline';
import type { ActivityItem } from '@/lib/task-activity';

enableAutoUnmount(afterEach);

function item(event_type: string, payload: unknown, id = event_type): ActivityItem {
  return {
    id,
    event_type,
    payload,
    created_at: new Date(2026, 10, 4, 9, 20).toISOString(),
    user: { id: 'user-1', name: 'yupix' },
  } as ActivityItem;
}

function mountEntry(items: ActivityItem[], lookups?: ActivityLookups, actor = 'yupix') {
  const entry: HistoryEntry = {
    kind: 'history',
    id: items[0]!.id,
    at: items[0]!.created_at,
    actor,
    items,
  };
  return mount(TaskActivityEntry, { props: { entry, lookups } });
}

describe('TaskActivityEntry', () => {
  it('主語と時刻を 1 度だけ出し、束ねた操作を行に並べる', () => {
    const wrapper = mountEntry([
      item('task_created', {}),
      item('status_changed', { from: 'Todo', to: 'Done' }),
    ]);

    expect(wrapper.text().match(/yupix/g)).toHaveLength(1);
    expect(wrapper.get('time').text()).toBe('9:20');
    expect(wrapper.findAll('[data-activity-line]')).toHaveLength(2);
  });

  it('変更前後を読み上げでも分かる形で出す', () => {
    const wrapper = mountEntry([item('status_changed', { from: 'Todo', to: 'Done' })]);
    expect(wrapper.get('[data-activity-line]').text()).toBe('ステータスTodo→からDone');
  });

  it('利用者が選んだ色は文字に使わず点に載せる', () => {
    const wrapper = mountEntry([item('status_changed', { to: 'Done' })], {
      statuses: [{ name: 'Done', color: '#eab308' }],
    });

    const colored = wrapper.findAll('[style*="color"]');
    expect(colored).toHaveLength(1);
    expect(colored[0]!.classes()).toContain('rounded-full');
    expect(colored[0]!.text()).toBe('');
  });

  it('優先度の色はアイコンだけに載せ、内部の値を出さない', () => {
    const wrapper = mountEntry([item('priority_changed', { from: 'medium', to: 'critical_fire' })]);

    expect(wrapper.text()).toContain('中');
    expect(wrapper.text()).toContain('緊急');
    expect(wrapper.text()).not.toContain('critical_fire');
    const colored = wrapper.findAll('[style*="color"]');
    expect(colored.map((el) => el.element.tagName.toLowerCase())).toEqual(['svg', 'svg']);
  });

  it('ラベルの増減を、追加か削除か分かる形で出す', () => {
    const wrapper = mountEntry([
      item('label_added', { label_id: 'l1', name: 'bug' }, 'a'),
      item('label_removed', { label_id: 'l1', name: 'bug' }, 'b'),
    ]);

    const [added, removed] = wrapper.findAll('[data-activity-line]');
    expect(added!.text()).toBe('ラベル＋追加bug');
    expect(removed!.text()).toBe('ラベル−削除bug');
  });

  describe('コミットと PR', () => {
    const sha = 'a3f92c1e7b81d4000000000000000000000000aa';

    it('短縮 SHA を新しいタブで開くリンクにし、メッセージを添える', () => {
      const url = `https://github.com/acme/backend/commit/${sha}`;
      const wrapper = mountEntry([
        item('forge_commit_linked', { sha, message: 'fix: 修正', html_url: url }),
      ]);

      const link = wrapper.get('a');
      expect(link.text()).toBe('a3f92c1');
      expect(link.attributes('href')).toBe(url);
      expect(link.attributes('target')).toBe('_blank');
      expect(link.attributes('rel')).toBe('noopener noreferrer');
      expect(link.classes()).toContain('font-mono');
      expect(wrapper.text()).toContain('fix: 修正');
    });

    it('http(s) 以外の URL はリンクにしない', () => {
      const wrapper = mountEntry([
        item('forge_commit_linked', { sha, message: 'fix', html_url: 'javascript:alert(1)' }),
      ]);
      expect(wrapper.find('a').exists()).toBe(false);
      expect(wrapper.text()).toContain('a3f92c1');
    });

    it('PR の番号をリンクにし、何が起きたかとタイトルを出す', () => {
      const url = 'https://github.com/acme/backend/pull/87';
      const wrapper = mountEntry([
        item('forge_pull_request_merged', { number: 87, title: 'feat: OAuth', html_url: url }),
      ]);

      expect(wrapper.get('a').text()).toBe('#87');
      expect(wrapper.get('a').attributes('href')).toBe(url);
      expect(wrapper.text()).toContain('マージで完了');
      expect(wrapper.text()).toContain('feat: OAuth');
    });
  });
});
