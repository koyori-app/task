import type { LucideIcon } from '@lucide/vue';

import type { CommentThread } from '@/composables/useTaskComments';
import {
  activityActor,
  activityText,
  commitActivity,
  priorityConfigOf,
  pullRequestActivity,
  type ActivityItem,
} from '@/lib/task-activity';

export type ActivityFilter = 'all' | 'comments' | 'history';

/** 同じ人の操作を 1 件に束ねる時間の幅。束の先頭から数える（連打で束が伸び続けないように） */
const GROUP_WINDOW_MS = 5 * 60_000;

export type HistoryEntry = {
  kind: 'history';
  id: string;
  at: string;
  actor: string;
  items: ActivityItem[];
};

export type CommentEntry = {
  kind: 'comment';
  id: string;
  at: string;
  thread: CommentThread;
};

export type TimelineEntry = HistoryEntry | CommentEntry;

export type TimelineSection = {
  key: string;
  label: string;
  entries: TimelineEntry[];
};

function dayKey(date: Date): string {
  return `${date.getFullYear()}-${date.getMonth() + 1}-${date.getDate()}`;
}

function dayLabel(date: Date, now: Date): string {
  if (dayKey(date) === dayKey(now)) return '今日';
  const yesterday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1);
  if (dayKey(date) === dayKey(yesterday)) return '昨日';
  const monthDay = `${date.getMonth() + 1}月${date.getDate()}日`;
  return date.getFullYear() === now.getFullYear() ? monthDay : `${date.getFullYear()}年${monthDay}`;
}

/** 日付の見出しの下に並ぶ時刻（`9:20`） */
export function timelineTime(iso: string): string {
  return new Date(iso).toLocaleTimeString('ja-JP', { hour: 'numeric', minute: '2-digit' });
}

/** 日付の見出しが無い場所（スレッド）の時刻（`11月3日 9:18`） */
export function timelineDateTime(iso: string, now: Date = new Date()): string {
  const date = new Date(iso);
  const monthDay = `${date.getMonth() + 1}月${date.getDate()}日`;
  const day =
    date.getFullYear() === now.getFullYear() ? monthDay : `${date.getFullYear()}年${monthDay}`;
  return `${day} ${timelineTime(iso)}`;
}

/** マージの履歴は作者の操作ではないので、作者を主語にしない */
function timelineActor(item: ActivityItem): string {
  return pullRequestActivity(item)?.merged && !item.user ? 'システム' : activityActor(item);
}

function actorKey(item: ActivityItem): string {
  return item.user?.id ?? timelineActor(item);
}

/**
 * 履歴とコメントを古い順の 1 本にまとめ、日付で区切る。
 *
 * 履歴はカーソルで少しずつ、コメントは全件取れる。未取得の履歴より古いコメントは
 * 前後関係が決まらないので、続きの履歴があるあいだは出さない（順序を偽らない）。
 */
export function buildTimeline(input: {
  threads: readonly CommentThread[];
  activities: readonly ActivityItem[];
  hasMoreActivities?: boolean;
  filter?: ActivityFilter;
  now?: Date;
}): TimelineSection[] {
  const filter = input.filter ?? 'all';
  const now = input.now ?? new Date();
  const time = (iso: string) => new Date(iso).getTime();

  // コメント本体が同じ列に出るので、投稿の履歴は重ねない
  const history =
    filter === 'comments'
      ? []
      : input.activities
          .filter((item) => item.event_type !== 'comment_added')
          .toSorted((a, b) => time(a.created_at) - time(b.created_at));

  let threads = filter === 'history' ? [] : [...input.threads];
  if (filter === 'all' && input.hasMoreActivities && input.activities.length) {
    const oldest = Math.min(...input.activities.map((item) => time(item.created_at)));
    threads = threads.filter((thread) => time(thread.created_at) >= oldest);
  }

  type Event =
    | { at: string; item: ActivityItem; thread?: never }
    | { at: string; thread: CommentThread; item?: never };
  const events: Event[] = [
    ...history.map((item) => ({ at: item.created_at, item })),
    ...threads.map((thread) => ({ at: thread.created_at, thread })),
  ].toSorted((a, b) => time(a.at) - time(b.at));

  const entries: TimelineEntry[] = [];
  for (const event of events) {
    if (event.thread) {
      entries.push({ kind: 'comment', id: event.thread.id, at: event.at, thread: event.thread });
      continue;
    }
    const last = entries.at(-1);
    if (
      last?.kind === 'history' &&
      actorKey(last.items[0]!) === actorKey(event.item) &&
      time(event.at) - time(last.at) <= GROUP_WINDOW_MS &&
      dayKey(new Date(last.at)) === dayKey(new Date(event.at))
    ) {
      last.items.push(event.item);
      continue;
    }
    entries.push({
      kind: 'history',
      id: event.item.id,
      at: event.at,
      actor: timelineActor(event.item),
      items: [event.item],
    });
  }

  const sections: TimelineSection[] = [];
  for (const entry of entries) {
    const date = new Date(entry.at);
    const key = dayKey(date);
    const section = sections.at(-1);
    if (section?.key === key) section.entries.push(entry);
    else sections.push({ key, label: dayLabel(date, now), entries: [entry] });
  }
  return sections;
}

/** ID や名前から表示を引く表。無ければ色や名前を省いて出す */
export type ActivityLookups = {
  statuses?: readonly { name: string; color: string }[];
  members?: readonly { id: string; username: string }[];
  labels?: readonly { id: string; name: string; color: string }[];
};

export type ActivityValue = { text: string; color?: string | null; icon?: LucideIcon };

export type ActivityLine =
  | { kind: 'change'; label: string; from: ActivityValue | null; to: ActivityValue }
  | { kind: 'membership'; label: string; added: boolean; value: ActivityValue }
  | {
      kind: 'link';
      label: string;
      text: string;
      url: string | null;
      mono: boolean;
      verb?: string;
      note: string;
    }
  | { kind: 'text'; text: string };

function priorityValue(raw: unknown): ActivityValue | null {
  if (typeof raw !== 'string') return null;
  const config = priorityConfigOf(raw);
  return config ? { text: config.label, color: config.color, icon: config.icon } : { text: raw };
}

function dateValue(raw: unknown): ActivityValue | null {
  return typeof raw === 'string' ? { text: raw.slice(0, 10).replaceAll('-', '/') } : null;
}

function textLine(item: ActivityItem): ActivityLine {
  const text = activityText(item);
  // 記録なので「〜しました」を省く。未知の履歴だけは「操作」1 語になるので文のまま出す
  if (text === '操作しました') return { kind: 'text', text };
  return { kind: 'text', text: text.replace(/を外しました$/, 'を解除').replace(/しました$/, '') };
}

/** 履歴 1 件を、列の幅に収まる「項目と値」の 1 行にする */
export function activityLine(item: ActivityItem, lookups: ActivityLookups = {}): ActivityLine {
  const payload = (item.payload ?? {}) as Record<string, unknown>;

  const commit = commitActivity(item);
  if (commit) {
    return {
      kind: 'link',
      label: 'コミット',
      text: commit.shortSha,
      url: commit.url,
      mono: true,
      note: commit.message,
    };
  }

  const pullRequest = pullRequestActivity(item);
  if (pullRequest) {
    const verb = pullRequest.merged
      ? 'マージで完了'
      : pullRequest.unlinked
        ? 'リンクを解除'
        : pullRequest.manual
          ? '手動でリンク'
          : undefined;
    return {
      kind: 'link',
      label: 'PR',
      text: pullRequest.number ? `#${pullRequest.number}` : 'PR',
      url: pullRequest.url,
      mono: false,
      verb,
      note: pullRequest.title,
    };
  }

  switch (item.event_type) {
    case 'status_changed': {
      const value = (raw: unknown): ActivityValue | null =>
        typeof raw === 'string'
          ? { text: raw, color: lookups.statuses?.find((s) => s.name === raw)?.color }
          : null;
      const to = value(payload.to);
      return to
        ? { kind: 'change', label: 'ステータス', from: value(payload.from), to }
        : textLine(item);
    }
    case 'priority_changed': {
      const to = priorityValue(payload.to);
      return to
        ? { kind: 'change', label: '優先度', from: priorityValue(payload.from), to }
        : textLine(item);
    }
    case 'deadline_changed':
      return {
        kind: 'change',
        label: payload.field === 'hard_deadline' ? 'ハード期限' : '期限',
        from: dateValue(payload.from),
        to: dateValue(payload.to) ?? { text: 'なし' },
      };
    case 'planned_start_changed':
      return {
        kind: 'change',
        label: '開始予定',
        from: dateValue(payload.from),
        to: dateValue(payload.to) ?? { text: 'なし' },
      };
    case 'assignee_added':
    case 'assignee_removed': {
      const member = lookups.members?.find((m) => m.id === payload.user_id);
      return member
        ? {
            kind: 'membership',
            label: '担当者',
            added: item.event_type === 'assignee_added',
            value: { text: member.username },
          }
        : textLine(item);
    }
    case 'label_added':
    case 'label_removed': {
      if (typeof payload.name !== 'string') return textLine(item);
      return {
        kind: 'membership',
        label: 'ラベル',
        added: item.event_type === 'label_added',
        value: {
          text: payload.name,
          color: lookups.labels?.find((l) => l.id === payload.label_id)?.color,
        },
      };
    }
    default:
      return textLine(item);
  }
}
