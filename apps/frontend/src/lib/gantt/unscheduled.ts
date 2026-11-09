import type { components } from '@/generated/api';
import { fetchClient } from '@/lib/api-vue-query';
import { matchesMembers, UNASSIGNED_MEMBER } from '@/lib/calendar/filters';

type UnscheduledResponse = components['schemas']['UnscheduledResponse'];
type Member = components['schemas']['UserSummary'];

export const UNSCHEDULED_PATH = '/v1/tenants/{tenant_id}/schedule/unscheduled' as const;
/** 一ページの数。期間の無い課題は多くないので、たいてい一ページで済む */
export const UNSCHEDULED_PAGE_LIMIT = 200;

type UnscheduledParams = {
  path: { tenant_id: string };
  query: {
    limit: number;
    /** プロジェクトで絞る（プロジェクトのガント）。見えないプロジェクトは API が無視する */
    project_id?: string[];
  };
};

/**
 * 期間の無い課題（開始予定日も締切も無い）を、next_cursor を辿って全件にする。
 * 権限の検めは予定の API と同じで、見えるプロジェクトの課題だけが返る。
 */
export async function fetchAllUnscheduled(
  { path, query }: UnscheduledParams,
  signal?: AbortSignal,
): Promise<UnscheduledResponse> {
  const fetchPage = async (cursor?: string) => {
    const { data, error } = await fetchClient.GET(UNSCHEDULED_PATH, {
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

/**
 * 選んだ担当者で絞った期間なしの課題。ガントの行と同じ絞り方にして、「期間なし N 件」が
 * 図に出ている課題と同じ人の分だけを数えるようにする。
 */
export function unscheduledFor<T extends { assignees: readonly { user_id: string }[] }>(
  tasks: readonly T[],
  memberIds: readonly string[],
): T[] {
  return tasks.filter((task) => matchesMembers(task, memberIds, { unassigned: UNASSIGNED_MEMBER }));
}

/**
 * ガントの二つの口（予定と期間なし）に付ける、プロジェクトの絞り。プロジェクトのガントでは
 * 両方を同じプロジェクトで絞り、他のプロジェクトの課題が図にも期間なしの数にも混ざらないようにする。
 */
export function projectScope(projectId: string | null | undefined): { project_id?: string[] } {
  return projectId ? { project_id: [projectId] } : {};
}
