import { describe, expect, it, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import { TooltipProvider } from 'reka-ui';
import { defineComponent, h } from 'vue';

vi.mock('vike/client/router', () => ({ navigate: vi.fn() }));

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
    logged_minutes: 0,
    assignees: [{ user_id: ALICE.id, role: 'primary' }],
    ...overrides,
  };
}

function mountTimeline(
  tasks: ScheduleTask[],
  members = [ALICE, BOB],
  extra: { loadTasks?: ScheduleTask[] } = {},
) {
  // TooltipProvider は本番では SidebarProvider が用意している（山の内訳が Tooltip を使う）
  const Host = defineComponent(
    () => () =>
      h(TooltipProvider, { delayDuration: 0 }, () =>
        h(CalendarTimeline, {
          from: '2026-10-12',
          today: '2026-10-14',
          timeZone: 'UTC',
          tenantSlug: 'acme',
          tasks,
          members,
          ...extra,
        }),
      ),
  );
  return mount(Host);
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
    // 上限までの棒と、上限の線を越えた警告色の棒（1h / 6h = 段の 1/6）
    expect(load.get('[data-load-bar]').attributes('style')).toContain('height: 1.5rem');
    const overflow = load.get('[data-load-overflow]');
    expect(overflow.classes()).toContain('bg-destructive/70');
    expect(overflow.attributes('style')).toContain('height: 0.25rem');
  });

  it('上限以内の日は警告色の棒を出さず、高さは上限に対する比率にする', () => {
    const wrapper = mountTimeline([
      task({
        planned_start_on: '2026-10-14',
        soft_deadline: '2026-10-14T00:00:00Z',
        estimated_minutes: 180,
      }),
    ]);
    const load = wrapper.get(`[data-member-row="${ALICE.id}"] [data-load-date="2026-10-14"]`);
    expect(load.attributes('data-over')).toBeUndefined();
    expect(load.get('[data-load-bar]').classes()).toContain('bg-primary/30');
    expect(load.get('[data-load-bar]').attributes('style')).toContain('height: 0.75rem');
    expect(load.find('[data-load-overflow]').exists()).toBe(false);
  });

  it('絞り込みで隠れたタスクも山に含める', () => {
    const shown = task({
      id: 't-shown',
      planned_start_on: '2026-10-14',
      soft_deadline: '2026-10-14T00:00:00Z',
      estimated_minutes: 240,
    });
    const hidden = task({
      id: 't-hidden',
      project: { id: 'p-2', key: 'OPS', name: 'Ops', is_personal: false },
      planned_start_on: '2026-10-14',
      soft_deadline: '2026-10-14T00:00:00Z',
      estimated_minutes: 180,
    });
    const all = mountTimeline([shown, hidden]);
    const filtered = mountTimeline([shown], [ALICE, BOB], { loadTasks: [shown, hidden] });
    const label = (w: ReturnType<typeof mountTimeline>) =>
      w
        .get(`[data-member-row="${ALICE.id}"] [data-load-date="2026-10-14"]`)
        .attributes('aria-label');
    expect(label(filtered)).toBe('2026-10-14 の見積 7h（上限 6h を超過）');
    expect(label(filtered)).toBe(label(all));
    // 帯は絞り込み後のタスクだけ
    expect(filtered.findAll(`[data-member-row="${ALICE.id}"] a`)).toHaveLength(1);
  });

  it('絞り込みで帯が 1 本も残らなくても、山があれば行を出す', () => {
    const hidden = task({
      planned_start_on: '2026-10-14',
      soft_deadline: '2026-10-14T00:00:00Z',
      estimated_minutes: 180,
    });
    const wrapper = mountTimeline([], [ALICE, BOB], { loadTasks: [hidden] });
    expect(wrapper.find('[data-calendar-empty]').exists()).toBe(false);
    expect(
      wrapper.find(`[data-member-row="${ALICE.id}"] [data-load-date="2026-10-14"]`).exists(),
    ).toBe(true);
    expect(wrapper.find(`[data-member-row="${ALICE.id}"] a`).exists()).toBe(false);
  });

  it('今日の列を aria-current で示す', () => {
    const wrapper = mountTimeline([]);
    expect(wrapper.get('[aria-current="date"]').attributes('data-date')).toBe('2026-10-14');
  });

  it('期間にタスクが無ければ空状態を出し、メンバーの行は出さない', () => {
    const wrapper = mountTimeline([]);
    expect(wrapper.find('[data-calendar-empty]').text()).toBe('この期間のタスクはありません');
    expect(wrapper.find('[data-member-row]').exists()).toBe(false);
  });
});
