import { describe, expect, it } from 'vitest';

import type { CommentThread } from '@/composables/useTaskComments';
import { activityLine, buildTimeline, timelineDateTime } from '@/lib/activity-timeline';
import type { ActivityItem } from '@/lib/task-activity';

// 日付の区切りは利用者のタイムゾーンで決まるので、固定値も地方時で作る
const at = (day: number, hour = 12, minute = 0, second = 0) =>
  new Date(2026, 10, day, hour, minute, second).toISOString();
const NOW = new Date(2026, 10, 4, 15, 0);

const yupix = { id: 'user-1', name: 'yupix' };
const tanaka = { id: 'user-2', name: '田中' };

function activity(id: string, created_at: string, overrides: Partial<ActivityItem> = {}) {
  return {
    id,
    created_at,
    event_type: 'status_changed',
    payload: { from: 'Todo', to: 'In Progress' },
    user: yupix,
    ...overrides,
  } as ActivityItem;
}

function thread(id: string, created_at: string): CommentThread {
  return {
    id,
    body: id,
    is_deleted: false,
    created_at,
    updated_at: created_at,
    user: tanaka,
    replies: [],
    reactions: [],
  } as CommentThread;
}

function ids(sections: ReturnType<typeof buildTimeline>) {
  return sections.flatMap((section) => section.entries.map((entry) => entry.id));
}

describe('buildTimeline', () => {
  it('新しい順の履歴と古い順のコメントを、古い順の 1 本にまとめる', () => {
    const sections = buildTimeline({
      // API は新しい順に返す
      activities: [activity('h3', at(4, 14)), activity('h2', at(3, 9)), activity('h1', at(2, 10))],
      threads: [thread('c1', at(2, 15)), thread('c2', at(4, 10))],
      now: NOW,
    });
    expect(ids(sections)).toEqual(['h1', 'c1', 'h2', 'c2', 'h3']);
  });

  it('コメント本体と重なる comment_added を除き、編集と削除の履歴は残す', () => {
    const sections = buildTimeline({
      activities: [
        activity('added', at(2), { event_type: 'comment_added', payload: {} }),
        activity('edited', at(3), { event_type: 'comment_edited', payload: {} }),
        activity('deleted', at(4), { event_type: 'comment_deleted', payload: {} }),
      ],
      threads: [],
      now: NOW,
    });
    expect(ids(sections)).toEqual(['edited', 'deleted']);
  });

  describe('束ね', () => {
    it('同じ人の 5 分ちょうどの操作は 1 件に束ね、古い順に並べる', () => {
      const sections = buildTimeline({
        activities: [activity('b', at(4, 10, 5)), activity('a', at(4, 10, 0))],
        threads: [],
        now: NOW,
      });
      expect(sections[0]!.entries).toHaveLength(1);
      const entry = sections[0]!.entries[0]!;
      expect(entry.kind === 'history' && entry.items.map((item) => item.id)).toEqual(['a', 'b']);
    });

    it('5 分を 1 秒でも超えたら束ねない', () => {
      const sections = buildTimeline({
        activities: [activity('b', at(4, 10, 5, 1)), activity('a', at(4, 10, 0))],
        threads: [],
        now: NOW,
      });
      expect(ids(sections)).toEqual(['a', 'b']);
    });

    it('束の幅は先頭から数える（連続しても 5 分を超えて伸びない）', () => {
      const sections = buildTimeline({
        activities: [
          activity('c', at(4, 10, 8)),
          activity('b', at(4, 10, 4)),
          activity('a', at(4, 10, 0)),
        ],
        threads: [],
        now: NOW,
      });
      expect(ids(sections)).toEqual(['a', 'c']);
    });

    it('別の人の操作は束ねない', () => {
      const sections = buildTimeline({
        activities: [activity('b', at(4, 10, 1), { user: tanaka }), activity('a', at(4, 10, 0))],
        threads: [],
        now: NOW,
      });
      expect(ids(sections)).toEqual(['a', 'b']);
    });

    it('間にコメントがあれば束ねない', () => {
      const sections = buildTimeline({
        activities: [activity('b', at(4, 10, 2)), activity('a', at(4, 10, 0))],
        threads: [thread('c', at(4, 10, 1))],
        now: NOW,
      });
      expect(ids(sections)).toEqual(['a', 'c', 'b']);
    });

    it('日をまたぐ操作は束ねない', () => {
      const sections = buildTimeline({
        activities: [activity('b', at(4, 0, 1)), activity('a', at(3, 23, 59))],
        threads: [],
        now: NOW,
      });
      expect(sections.map((section) => section.label)).toEqual(['昨日', '今日']);
    });
  });

  describe('続きの履歴があるとき', () => {
    const activities = [activity('h2', at(4, 12)), activity('h1', at(3, 12))];
    const threads = [thread('old', at(2, 12)), thread('new', at(3, 15))];

    it('取得済みの最も古い履歴より古いコメントを出さない', () => {
      const sections = buildTimeline({ activities, threads, hasMoreActivities: true, now: NOW });
      expect(ids(sections)).toEqual(['h1', 'new', 'h2']);
    });

    it('履歴を取り切ったらすべてのコメントを出す', () => {
      const sections = buildTimeline({ activities, threads, hasMoreActivities: false, now: NOW });
      expect(ids(sections)).toEqual(['old', 'h1', 'new', 'h2']);
    });

    it('コメントの絞り込みでは履歴の取得範囲に関わらずすべて出す', () => {
      const sections = buildTimeline({
        activities,
        threads,
        hasMoreActivities: true,
        filter: 'comments',
        now: NOW,
      });
      expect(ids(sections)).toEqual(['old', 'new']);
    });
  });

  it('履歴の絞り込みではコメントを出さない', () => {
    const sections = buildTimeline({
      activities: [activity('h1', at(3))],
      threads: [thread('c1', at(4))],
      filter: 'history',
      now: NOW,
    });
    expect(ids(sections)).toEqual(['h1']);
  });

  it('日付の見出しは今日、昨日、月日、年つきの順に変わる', () => {
    const sections = buildTimeline({
      activities: [
        activity('today', at(4)),
        activity('yesterday', at(3)),
        activity('this-year', at(1)),
        activity('last-year', new Date(2025, 11, 31, 12).toISOString()),
      ],
      threads: [],
      now: NOW,
    });
    expect(sections.map((section) => section.label)).toEqual([
      '2025年12月31日',
      '11月1日',
      '昨日',
      '今日',
    ]);
  });

  it('マージの履歴は作者を主語にしない', () => {
    const sections = buildTimeline({
      activities: [
        activity('merged', at(4), {
          event_type: 'forge_pull_request_merged',
          user: null,
          payload: { number: 87, author_handle: 'yupix' },
        }),
      ],
      threads: [],
      now: NOW,
    });
    const entry = sections[0]!.entries[0]!;
    expect(entry.kind === 'history' && entry.actor).toBe('システム');
  });
});

describe('timelineDateTime', () => {
  it('同じ年は月日と時刻、別の年は年も出す', () => {
    expect(timelineDateTime(at(3, 9, 18), NOW)).toBe('11月3日 9:18');
    expect(timelineDateTime(new Date(2025, 0, 2, 14, 5).toISOString(), NOW)).toBe(
      '2025年1月2日 14:05',
    );
  });
});

describe('activityLine', () => {
  const line = (
    event_type: string,
    payload: unknown,
    lookups?: Parameters<typeof activityLine>[1],
  ) => activityLine(activity('x', at(4), { event_type, payload }), lookups);

  it('ステータスは変更前後の名前と、プロジェクトの色を出す', () => {
    expect(
      line(
        'status_changed',
        { from: 'Todo', to: 'Done' },
        {
          statuses: [
            { name: 'Todo', color: '#111111' },
            { name: 'Done', color: '#222222' },
          ],
        },
      ),
    ).toEqual({
      kind: 'change',
      label: 'ステータス',
      from: { text: 'Todo', color: '#111111' },
      to: { text: 'Done', color: '#222222' },
    });
  });

  it('色を引けないステータスも名前は出す', () => {
    expect(line('status_changed', { to: 'Done' })).toMatchObject({
      from: null,
      to: { text: 'Done', color: undefined },
    });
  });

  it('優先度は内部の値を出さず、日本語の名前と色にする', () => {
    expect(line('priority_changed', { from: 'critical_fire', to: 'high' })).toMatchObject({
      label: '優先度',
      from: { text: '緊急', color: '#dc2626' },
      to: { text: '高', color: '#f97316' },
    });
  });

  it.each([
    ['critical_fire', '緊急'],
    ['critical', '重大'],
    ['high', '高'],
    ['medium', '中'],
    ['low', '低'],
    ['trivial', '些細'],
    // 表にない値は隠さずそのまま出す
    ['urgent', 'urgent'],
  ])('優先度 %s を %s と出す', (to, text) => {
    expect(line('priority_changed', { from: 'low', to })).toMatchObject({ to: { text } });
  });

  it.each([
    ['label_added', 'ラベルを追加'],
    ['label_removed', 'ラベルを解除'],
  ])('名前の無い %s は文で出す', (event_type, text) => {
    expect(line(event_type, {})).toEqual({ kind: 'text', text });
  });

  it.each([
    [{ field: 'soft_deadline', from: '2026-10-05T00:00:00Z', to: '2026-10-07T00:00:00Z' }, '期限'],
    [{ field: 'hard_deadline', from: '2026-10-05T00:00:00Z', to: null }, 'ハード期限'],
  ])('期限は変更前後の日付を出す: %j', (payload, label) => {
    const result = line('deadline_changed', payload);
    expect(result).toMatchObject({ kind: 'change', label, from: { text: '2026/10/05' } });
    expect(result).toMatchObject({ to: { text: payload.to ? '2026/10/07' : 'なし' } });
  });

  it('開始予定日を解除した履歴は「なし」にする', () => {
    expect(line('planned_start_changed', { from: '2026-10-05', to: null })).toMatchObject({
      label: '開始予定',
      to: { text: 'なし' },
    });
  });

  it('担当者はメンバー一覧から名前を引く', () => {
    expect(
      line('assignee_added', { user_id: 'u1' }, { members: [{ id: 'u1', username: '田中' }] }),
    ).toEqual({ kind: 'membership', label: '担当者', added: true, value: { text: '田中' } });
  });

  it('メンバーを引けない担当者は文で出す', () => {
    expect(line('assignee_removed', { user_id: 'gone' })).toEqual({
      kind: 'text',
      text: '担当者を解除',
    });
  });

  it('ラベルの増減を名前と色で出す', () => {
    expect(
      line(
        'label_removed',
        { label_id: 'l1', name: 'bug' },
        { labels: [{ id: 'l1', name: 'bug', color: '#e11d48' }] },
      ),
    ).toEqual({
      kind: 'membership',
      label: 'ラベル',
      added: false,
      value: { text: 'bug', color: '#e11d48' },
    });
  });

  it('コミットは短縮 SHA とメッセージを出し、http(s) 以外の URL はリンクにしない', () => {
    const sha = 'a3f92c1e7b81d4000000000000000000000000aa';
    expect(
      line('forge_commit_linked', { sha, message: 'fix', html_url: 'https://example.com/c' }),
    ).toEqual({
      kind: 'link',
      label: 'コミット',
      text: 'a3f92c1',
      url: 'https://example.com/c',
      mono: true,
      note: 'fix',
    });
    expect(
      line('forge_commit_linked', { sha, message: 'fix', html_url: 'javascript:alert(1)' }),
    ).toMatchObject({ url: null });
  });

  it.each([
    ['forge_pull_request_linked', {}, undefined],
    ['forge_pull_request_linked', { source: 'manual' }, '手動でリンク'],
    ['forge_pull_request_unlinked', {}, 'リンクを解除'],
    ['forge_pull_request_merged', {}, 'マージで完了'],
  ])('PR の履歴: %s %j', (event_type, extra, verb) => {
    expect(
      line(event_type, { number: 87, title: 'feat', html_url: 'https://x/87', ...extra }),
    ).toEqual({
      kind: 'link',
      label: 'PR',
      text: '#87',
      url: 'https://x/87',
      mono: false,
      verb,
      note: 'feat',
    });
  });

  it.each([
    ['task_created', {}, 'タスクを作成'],
    ['archived', {}, 'タスクをアーカイブ'],
    ['comment_edited', {}, 'コメントを編集'],
    [
      'github_issue_synced',
      { repo_owner: 'acme', repo_name: 'backend', issue_number: 42 },
      'GitHub Issue（acme/backend#42）からタスクを同期',
    ],
    ['github_issue_synced', null, 'GitHub Issueからタスクを同期'],
    [
      'forge_merge_effects_acknowledged',
      { number: 87 },
      'PR #87 の自動完了を未適用のまま確認済みに',
    ],
  ])('文で出す履歴: %s', (event_type, payload, text) => {
    expect(line(event_type, payload)).toEqual({ kind: 'text', text });
  });

  it('未知の履歴は欠かさず文のまま出す', () => {
    expect(line('something_new', {})).toEqual({ kind: 'text', text: '操作しました' });
  });
});
