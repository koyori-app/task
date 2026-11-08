import type { components } from '@/generated/api';
import { fetchClient } from '@/lib/api-vue-query';

type ScheduleResponse = components['schemas']['ScheduleResponse'];
type Member = components['schemas']['UserSummary'];

export const SCHEDULE_PATH = '/v1/tenants/{tenant_id}/schedule' as const;
/** API の上限。14 日なら実用上 1〜2 ページで済む */
export const SCHEDULE_PAGE_LIMIT = 500;

type ScheduleParams = {
  path: { tenant_id: string };
  query: {
    from: string;
    to: string;
    timezone: string;
    limit: number;
    include_overdue: boolean;
  };
};

/**
 * next_cursor を辿って全件にする。スプリント等は全ページ同じなので先頭ページのものを使う。
 * カレンダーとガントが同じ口・同じ束ね方で引くよう、ここに一つだけ置く。
 */
export async function fetchAllSchedule(
  { path, query }: ScheduleParams,
  signal?: AbortSignal,
): Promise<ScheduleResponse> {
  const fetchPage = async (cursor?: string) => {
    const { data, error } = await fetchClient.GET(SCHEDULE_PATH, {
      params: { path, query: { ...query, cursor } },
      signal,
    });
    if (error) throw error;
    return data;
  };
  const first = await fetchPage();
  const tasks = [...first.tasks];
  const members = new Map<string, Member>(first.members.map((m) => [m.id, m]));
  for (let cursor = first.next_cursor; cursor;) {
    const page = await fetchPage(cursor);
    tasks.push(...page.tasks);
    for (const member of page.members) members.set(member.id, member);
    cursor = page.next_cursor;
  }
  return { ...first, tasks, members: [...members.values()], next_cursor: null };
}
