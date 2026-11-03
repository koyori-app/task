import { describe, expect, it } from 'vitest';

import {
  matchesMembers,
  memberDropdownItems,
  membersAssignedTo,
  toggled,
  UNASSIGNED_MEMBER,
} from '../filters';

const task = (...userIds: string[]) => ({ assignees: userIds.map((user_id) => ({ user_id })) });

describe('matchesMembers', () => {
  it('何も選ばなければ、担当の有無に関わらず全て通す', () => {
    expect(matchesMembers(task('a'), [])).toBe(true);
    expect(matchesMembers(task(), [])).toBe(true);
  });

  it('一人選べば、その者が担当する課題だけ通す', () => {
    expect(matchesMembers(task('a'), ['a'])).toBe(true);
    expect(matchesMembers(task('b'), ['a'])).toBe(false);
  });

  it('二人選べば、どちらかが担当する課題を通す', () => {
    expect(matchesMembers(task('a'), ['a', 'b'])).toBe(true);
    expect(matchesMembers(task('b'), ['a', 'b'])).toBe(true);
    expect(matchesMembers(task('c'), ['a', 'b'])).toBe(false);
  });

  it('二人以上が担当する課題は、そのうち誰を選んでも通す（全員を選ぶ必要は無い）', () => {
    const shared = task('a', 'b');
    expect(matchesMembers(shared, ['a'])).toBe(true);
    expect(matchesMembers(shared, ['b'])).toBe(true);
    expect(matchesMembers(shared, ['a', 'b'])).toBe(true);
    expect(matchesMembers(shared, ['c'])).toBe(false);
  });

  it('担当の無い課題は、印を許した上で印を選んだ時だけ通す', () => {
    const options = { unassigned: UNASSIGNED_MEMBER };
    expect(matchesMembers(task(), [UNASSIGNED_MEMBER], options)).toBe(true);
    expect(matchesMembers(task(), [UNASSIGNED_MEMBER, 'a'], options)).toBe(true);
    expect(matchesMembers(task(), ['a'], options)).toBe(false);
  });

  it('印を選んでも、担当のある課題は印だけでは通らない', () => {
    expect(matchesMembers(task('a'), [UNASSIGNED_MEMBER], { unassigned: UNASSIGNED_MEMBER })).toBe(
      false,
    );
  });

  it('印を許さなければ、印が URL に入っていても担当の無い課題は通さない', () => {
    expect(matchesMembers(task(), [UNASSIGNED_MEMBER])).toBe(false);
  });
});

describe('membersAssignedTo', () => {
  const members = [
    { id: 'a', username: 'alice' },
    { id: 'b', username: 'bob' },
    { id: 'c', username: 'carol' },
  ];

  it('課題に担当として現れる者だけを、元の並びで残す', () => {
    expect(membersAssignedTo([task('c'), task('a', 'c'), task()], members)).toEqual([
      members[0],
      members[2],
    ]);
  });

  it('課題が無ければ誰も残らない', () => {
    expect(membersAssignedTo([], members)).toEqual([]);
  });
});

describe('memberDropdownItems', () => {
  it('選んだ者に印を付けた checkbox にする', () => {
    expect(memberDropdownItems([{ id: 'a', username: 'alice' }], ['a'])).toEqual([
      { value: 'a', label: 'alice', kind: 'checkbox', checked: true },
    ]);
    expect(memberDropdownItems([{ id: 'a', username: 'alice' }], [])[0]!.checked).toBe(false);
  });
});

describe('toggled', () => {
  it('無ければ足し、有れば外す', () => {
    expect(toggled(['a'], 'b')).toEqual(['a', 'b']);
    expect(toggled(['a', 'b'], 'a')).toEqual(['b']);
  });
});
