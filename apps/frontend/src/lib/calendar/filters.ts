import type { DropdownItem } from '@koyori-app/ui-vue';

import type { components } from '@/generated/api';

type Member = components['schemas']['UserSummary'];
type Assigned = { assignees: readonly { user_id: string }[] };

/**
 * 「担当者が一人も居ない課題」を `member` クエリで指すための印。
 * ユーザー ID は UUID なので、この字とは重ならない。
 */
export const UNASSIGNED_MEMBER = 'unassigned';

/** 選び肢に出す者。見ている範囲の課題に担当として現れる者だけを残す */
export function membersAssignedTo<T extends { id: string }>(
  tasks: readonly Assigned[],
  members: readonly T[],
): T[] {
  const assigned = new Set(tasks.flatMap((task) => task.assignees.map((a) => a.user_id)));
  return members.filter((member) => assigned.has(member.id));
}

/**
 * 選んだ者のうち一人でも担当している課題か。何も選んでいなければ全て通す。
 * `unassigned` を渡した時だけ、その印が選ばれていれば担当の無い課題も通す
 * （渡さなければ、担当の無い課題は誰かを選んだ時点で外れる）。
 */
export function matchesMembers(
  task: Assigned,
  memberIds: readonly string[],
  options: { unassigned?: string } = {},
): boolean {
  if (!memberIds.length) return true;
  if (!task.assignees.length) {
    return options.unassigned !== undefined && memberIds.includes(options.unassigned);
  }
  return task.assignees.some((a) => memberIds.includes(a.user_id));
}

/** 絞り込みは checkbox。選んでもメニューは閉じないので、続けて複数を選べる */
export function memberDropdownItems(
  members: readonly Pick<Member, 'id' | 'username'>[],
  selectedIds: readonly string[],
): DropdownItem[] {
  return members.map((member) => ({
    value: member.id,
    label: member.username,
    kind: 'checkbox',
    checked: selectedIds.includes(member.id),
  }));
}

/** 一覧に入っていれば外し、無ければ足す */
export function toggled(list: string[], id: string): string[] {
  return list.includes(id) ? list.filter((v) => v !== id) : [...list, id];
}
