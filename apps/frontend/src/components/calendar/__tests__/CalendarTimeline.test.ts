import { describe, expect, it } from 'vitest';
import { mount } from '@vue/test-utils';

import CalendarTimeline from '../CalendarTimeline.vue';
import type { components } from '@/generated/api';

type ScheduleTask = components['schemas']['ScheduleTask'];

const ALICE = { id: 'u-alice', username: 'alice' };
const BOB = { id: 'u-bob', username: 'bob' };

function task(overrides: Partial<ScheduleTask>): ScheduleTask {
  return {
    id: 't-1',
    seq_id: 1,
    seq_key: 'ENG-1',
    title: 'タスク',
    project: { id: 'p-1', key: 'ENG', name: 'Engineering', is_personal: false },
    status: { id: 's-1', name: 'Todo', color: '#888', is_done_state: false },
    priority: 'Medium',
    progress_pct: 0,
    assignees: [{ user_id: ALICE.id, role: 'primary' }],
    ...overrides,
  };
}

function mountTimeline(tasks: ScheduleTask[], members = [ALICE, BOB]) {
  return mount(CalendarTimeline, {
    props: {
      from: '2026-10-12',
      today: '2026-10-14',
      timeZone: 'UTC',
      tenantSlug: 'acme',
      tasks,
      members,
    },
  });
}

describe('CalendarTimeline', () => {
  it('主担当は塗りの帯、サブ担当は枠線だけの帯で同じタスクを両方の行に描く', () => {
    const wrapper = mountTimeline([
      task({
        planned_start_on: '2026-10-13',
        soft_deadline: '2026-10-15T00:00:00Z',
        assignees: [
          { user_id: ALICE.id, role: 'primary' },
          { user_id: BOB.id, role: 'assignee' },
        ],
      }),
    ]);
    const alice = wrapper.get(`[data-member-row="${ALICE.id}"] a`);
    const bob = wrapper.get(`[data-member-row="${BOB.id}"] a`);
    expect(alice.attributes('data-role')).toBe('primary');
    expect(alice.classes()).toContain('bg-primary/15');
    expect(bob.attributes('data-role')).toBe('sub');
    expect(bob.classes()).toContain('border-dashed');
    // 火〜木 = 列 1〜3（1 列目はメンバー名なので +2）
    expect(alice.attributes('style')).toContain('grid-column: 3 / 6');
    expect(alice.attributes('href')).toBe('/acme/projects/ENG/tasks/ENG-1');
  });

  it('開始予定日の無いタスクは締切の点で描く', () => {
    const wrapper = mountTimeline([task({ soft_deadline: '2026-10-20T00:00:00Z' })]);
    const entry = wrapper.get(`[data-member-row="${ALICE.id}"] a`);
    expect(entry.attributes('data-shape')).toBe('point');
    expect(entry.find('.rotate-45').exists()).toBe(true);
  });

  it('範囲をはみ出す帯は端で切り詰めて矢印を出す', () => {
    const wrapper = mountTimeline([
      task({ planned_start_on: '2026-10-01', soft_deadline: '2026-10-13T00:00:00Z' }),
    ]);
    const entry = wrapper.get(`[data-member-row="${ALICE.id}"] a`);
    expect(entry.attributes('style')).toContain('grid-column: 2 / 4');
    expect(entry.findAll('svg')).toHaveLength(1);
  });

  it('上限を超えた日に超過の印と読み上げ文言を出す', () => {
    const wrapper = mountTimeline([
      task({
        planned_start_on: '2026-10-14',
        soft_deadline: '2026-10-14T00:00:00Z',
        estimated_minutes: 420,
      }),
    ]);
    const load = wrapper.get(`[data-member-row="${ALICE.id}"] [data-load-date="2026-10-14"]`);
    expect(load.attributes('data-over')).toBe('true');
    expect(load.attributes('aria-label')).toBe('2026-10-14 の見積 7h（上限 6h を超過）');
    expect(load.text()).toContain('超過');
  });

  it('帯の左クリックは遷移させず open-task を emit し、修飾キー付きはブラウザに任せる', async () => {
    const wrapper = mountTimeline([task({ soft_deadline: '2026-10-20T00:00:00Z' })]);
    const entry = wrapper.get(`[data-member-row="${ALICE.id}"] a`);

    const plain = new MouseEvent('click', { bubbles: true, cancelable: true });
    entry.element.dispatchEvent(plain);
    expect(plain.defaultPrevented).toBe(true);
    expect(wrapper.emitted('open-task')?.[0]?.[0]).toMatchObject({ seq_key: 'ENG-1' });

    const withMeta = new MouseEvent('click', { bubbles: true, cancelable: true, metaKey: true });
    entry.element.dispatchEvent(withMeta);
    expect(withMeta.defaultPrevented).toBe(false);
    expect(wrapper.emitted('open-task')).toHaveLength(1);
  });

  it('今日の列を aria-current で示す', () => {
    const wrapper = mountTimeline([]);
    expect(wrapper.get('[aria-current="date"]').attributes('data-date')).toBe('2026-10-14');
  });

  it('メンバーの行の見出しに担当者のアバターを出し、画像が無ければ頭文字にする', () => {
    const wrapper = mountTimeline([task({ soft_deadline: '2026-10-14T00:00:00Z' })]);
    const avatar = wrapper.get(`[data-member-row="${ALICE.id}"] [data-member-avatar]`);
    expect(avatar.text()).toBe('A');
    // 名前が隣にあるので読み上げない
    expect(avatar.attributes('aria-hidden')).toBe('true');
    expect(wrapper.get(`[data-member-row="${BOB.id}"] [data-member-avatar]`).text()).toBe('B');
    // 画像が無い行の頭文字は 1 文字、色は UserAvatar の組
    expect(avatar.classes()).toContain('size-5');
    expect(avatar.get('[data-slot=avatar-fallback]').classes()).toContain('text-foreground');
  });

  it('担当者なしの行は頭文字ではなく人のアイコンを出す', () => {
    const wrapper = mountTimeline([task({ soft_deadline: '2026-10-14T00:00:00Z', assignees: [] })]);
    const avatar = wrapper.get('[data-member-row="__unassigned__"] [data-member-avatar]');
    expect(avatar.text()).toBe('');
    expect(avatar.find('svg').exists()).toBe(true);
    // 地は bg-muted、字（アイコン）は継ぐ色の text-foreground。薄い text-muted-foreground は使わぬ
    const fallback = avatar.get('[data-slot=avatar-fallback]');
    expect(fallback.classes()).toEqual(expect.arrayContaining(['bg-muted', 'text-foreground']));
    expect(avatar.find('svg').classes()).not.toContain('text-muted-foreground');
    expect(avatar.attributes('aria-hidden')).toBe('true');
    expect(avatar.classes()).toContain('size-5');
  });

  it('期間にタスクが無ければ空状態を出し、メンバーの行は出さない', () => {
    const wrapper = mountTimeline([]);
    expect(wrapper.find('[data-calendar-empty]').text()).toBe('この期間のタスクはありません');
    expect(wrapper.find('[data-member-row]').exists()).toBe(false);
  });
});
