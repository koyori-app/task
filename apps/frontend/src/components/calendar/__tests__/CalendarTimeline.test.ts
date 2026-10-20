import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { TooltipProvider } from 'reka-ui';
import { defineComponent, h } from 'vue';

vi.mock('vike/client/router', () => ({ navigate: vi.fn() }));
const { put } = vi.hoisted(() => ({ put: vi.fn() }));
vi.mock('@/lib/api-vue-query', () => ({ fetchClient: { PUT: put } }));

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
  extra: { loadTasks?: ScheduleTask[]; tenantId?: string } = {},
  attachTo?: HTMLElement,
) {
  const queryClient = new QueryClient();
  const invalidate = vi.spyOn(queryClient, 'invalidateQueries');
  // TooltipProvider は本番では SidebarProvider が用意している（山の内訳が Tooltip を使う）
  const Host = defineComponent(
    () => () =>
      h(TooltipProvider, { delayDuration: 0 }, () =>
        h(CalendarTimeline, {
          from: '2026-10-12',
          today: '2026-10-14',
          timeZone: 'UTC',
          tenantSlug: 'acme',
          tenantId: 'tenant-1',
          tasks,
          members,
          ...extra,
        }),
      ),
  );
  const wrapper = mount(Host, {
    attachTo,
    global: { plugins: [[VueQueryPlugin, { queryClient }]] },
  });
  return Object.assign(wrapper, { invalidate });
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
    expect(load.attributes('aria-label')).toBe('2026-10-14 の残り 7h（上限 6h を超過）');
    expect(load.text()).toContain('超過');
    // 上限までの棒と、上限の線を越えた警告色の棒（1h / 6h = 段の 1/6）
    expect(load.get('[data-load-bar]').attributes('style')).toContain('height: 1.5rem');
    const overflow = load.get('[data-load-overflow]');
    expect(overflow.classes()).toContain('bg-destructive/70');
    expect(overflow.attributes('style')).toContain('height: 0.25rem');
  });

  it('山にキーボードでフォーカスすると内訳を出す', async () => {
    const wrapper = mountTimeline([
      task({
        planned_start_on: '2026-10-14',
        soft_deadline: '2026-10-14T00:00:00Z',
        estimated_minutes: 420,
        title: '設計レビュー',
      }),
    ]);
    const load = wrapper.get(`[data-member-row="${ALICE.id}"] [data-load-date="2026-10-14"]`);
    expect(load.attributes('tabindex')).toBe('0');
    await load.trigger('focus');
    await vi.waitFor(() => {
      const items = document.body.querySelector('[data-load-items]');
      expect(items?.textContent).toContain('ENG-1');
      expect(items?.textContent).toContain('設計レビュー');
      expect(items?.textContent).toContain('7h');
    });
    wrapper.unmount();
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
    expect(label(filtered)).toBe('2026-10-14 の残り 7h（上限 6h を超過）');
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

  describe('開始予定日の移動', () => {
    // 火〜木の 3 列の帯。1 列 = 100px
    const movable = () =>
      task({
        planned_start_on: '2026-10-13',
        soft_deadline: '2026-10-15T00:00:00Z',
        estimated_minutes: 360,
      });
    const bar = (w: ReturnType<typeof mountTimeline>) =>
      w.get(`[data-member-row="${ALICE.id}"] [data-task-id="t-1"]`);
    const message = (w: ReturnType<typeof mountTimeline>) => w.find('[data-move-message]');

    beforeEach(() => {
      put.mockReset();
      vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({
        width: 300,
      } as DOMRect);
    });
    afterEach(() => vi.restoreAllMocks());

    it('ドラッグで始点だけを動かし、山も追従させてから planned_start_on だけを送る', async () => {
      put.mockResolvedValue({ data: {}, error: undefined, response: { status: 200 } });
      // 木〜翌火（平日 4 日）に 8h = 2h/日。帯は 6 列なので 1 列 = 50px
      const wrapper = mountTimeline([
        task({
          planned_start_on: '2026-10-15',
          soft_deadline: '2026-10-20T00:00:00Z',
          estimated_minutes: 480,
        }),
      ]);
      const load = (date: string) =>
        wrapper.find(`[data-member-row="${ALICE.id}"] [data-load-date="${date}"]`);
      expect(load('2026-10-14').exists()).toBe(false);
      await bar(wrapper).trigger('pointerdown', { clientX: 500, button: 0 });
      await bar(wrapper).trigger('pointermove', { clientX: 450 });
      // 水〜翌火（終点の締切は動かない）。確定前でも山は平日 5 日の 1.6h/日に変わる
      expect(bar(wrapper).attributes('style')).toContain('grid-column: 4 / 11');
      expect(load('2026-10-14').attributes('aria-label')).toBe('2026-10-14 の残り 1.6h');
      expect(put).not.toHaveBeenCalled();

      await bar(wrapper).trigger('pointerup');
      await flushPromises();
      expect(put).toHaveBeenCalledWith('/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}', {
        params: { path: { tenant_id: 'tenant-1', project_id: 'p-1', id: 't-1' } },
        body: { planned_start_on: '2026-10-14' },
      });
      expect(wrapper.invalidate).toHaveBeenCalledWith({
        queryKey: ['get', '/v1/tenants/{tenant_id}/schedule'],
      });
    });

    it('キーボードの ← → で 1 日ずつ動かし、Enter で確定、Esc で取り消す', async () => {
      put.mockResolvedValue({ data: {}, error: undefined, response: { status: 200 } });
      const wrapper = mountTimeline([movable()]);
      await bar(wrapper).trigger('keydown', { key: 'ArrowRight' });
      await bar(wrapper).trigger('keydown', { key: 'ArrowLeft' });
      await bar(wrapper).trigger('keydown', { key: 'ArrowLeft' });
      expect(bar(wrapper).attributes('style')).toContain('grid-column: 2 / 6');
      await bar(wrapper).trigger('keydown', { key: 'Escape' });
      expect(bar(wrapper).attributes('style')).toContain('grid-column: 3 / 6');
      expect(put).not.toHaveBeenCalled();

      await bar(wrapper).trigger('keydown', { key: 'ArrowLeft' });
      await bar(wrapper).trigger('keydown', { key: 'Enter' });
      await flushPromises();
      expect(put).toHaveBeenCalledTimes(1);
      expect(put.mock.calls[0]![1].body).toEqual({ planned_start_on: '2026-10-12' });
    });

    it('点（開始予定日の無いタスク）はドラッグでもキーボードでも動かせない', async () => {
      const wrapper = mountTimeline([task({ soft_deadline: '2026-10-15T00:00:00Z' })]);
      const point = bar(wrapper);
      expect(point.attributes('data-shape')).toBe('point');
      await point.trigger('pointerdown', { clientX: 500, button: 0 });
      await point.trigger('pointermove', { clientX: 300 });
      await point.trigger('pointerup');
      await point.trigger('keydown', { key: 'ArrowLeft' });
      await point.trigger('keydown', { key: 'Enter' });
      await flushPromises();
      expect(point.attributes('data-moving')).toBeUndefined();
      expect(point.attributes('style')).toContain('grid-column: 5 / 6');
      expect(put).not.toHaveBeenCalled();
    });

    it('Soft 締切より後へドロップすると警告して送らず、元の位置に戻す', async () => {
      const wrapper = mountTimeline([movable()]);
      await bar(wrapper).trigger('pointerdown', { clientX: 0, button: 0 });
      await bar(wrapper).trigger('pointermove', { clientX: 300 });
      expect(message(wrapper).text()).toBe('期限より後になっています');
      await bar(wrapper).trigger('pointerup');
      await flushPromises();
      expect(put).not.toHaveBeenCalled();
      expect(bar(wrapper).attributes('style')).toContain('grid-column: 3 / 6');
      expect(message(wrapper).text()).toBe('開始予定日が期限より後になるため動かせません');
    });

    it('締切ちょうどまでは置ける', async () => {
      put.mockResolvedValue({ data: {}, error: undefined, response: { status: 200 } });
      const wrapper = mountTimeline([movable()]);
      await bar(wrapper).trigger('keydown', { key: 'ArrowRight' });
      await bar(wrapper).trigger('keydown', { key: 'ArrowRight' });
      expect(message(wrapper).exists()).toBe(false);
      await bar(wrapper).trigger('keydown', { key: 'Enter' });
      await flushPromises();
      expect(put.mock.calls[0]![1].body).toEqual({ planned_start_on: '2026-10-15' });
    });

    it.each([
      [403, 'このタスクは変更できません'],
      [404, 'このタスクは変更できません'],
      [500, '開始予定日を変更できませんでした（boom）'],
    ])('%s で失敗したら元の位置に戻して「%s」を出す', async (status, text) => {
      put.mockResolvedValue({ error: { message: 'boom' }, response: { status } });
      const wrapper = mountTimeline([movable()]);
      await bar(wrapper).trigger('keydown', { key: 'ArrowLeft' });
      await bar(wrapper).trigger('keydown', { key: 'Enter' });
      await flushPromises();
      expect(put).toHaveBeenCalledTimes(1);
      expect(bar(wrapper).attributes('style')).toContain('grid-column: 3 / 6');
      expect(message(wrapper).text()).toBe(text);
      expect(wrapper.invalidate).not.toHaveBeenCalled();
    });

    it('送信中は Enter で二重に送らず、Esc でも見た目だけ戻さない', async () => {
      let resolve!: (value: unknown) => void;
      put.mockReturnValue(new Promise((r) => (resolve = r)));
      const wrapper = mountTimeline([movable()]);
      await bar(wrapper).trigger('keydown', { key: 'ArrowLeft' });
      await bar(wrapper).trigger('keydown', { key: 'Enter' });
      await bar(wrapper).trigger('keydown', { key: 'Enter' });
      await bar(wrapper).trigger('keydown', { key: 'Escape' });
      await bar(wrapper).trigger('keydown', { key: 'ArrowLeft' });
      // 送信後のタッチスクロールなどによる取消でも見た目だけ戻さない
      await bar(wrapper).trigger('pointercancel');
      await bar(wrapper).trigger('lostpointercapture');
      expect(put).toHaveBeenCalledTimes(1);
      expect(bar(wrapper).attributes('style')).toContain('grid-column: 2 / 6');
      resolve({ data: {}, error: undefined, response: { status: 200 } });
      await flushPromises();
      expect(put).toHaveBeenCalledTimes(1);
      expect(wrapper.invalidate).toHaveBeenCalledTimes(1);
    });

    it('帯が表示範囲の外へ出る位置には動かさない', async () => {
      // 締切の無い 1 日だけの帯（範囲の初日）
      const wrapper = mountTimeline([task({ planned_start_on: '2026-10-12' })]);
      await bar(wrapper).trigger('keydown', { key: 'ArrowLeft' });
      expect(bar(wrapper).attributes('data-moving')).toBeUndefined();
      expect(bar(wrapper).attributes('style')).toContain('grid-column: 2 / 3');
      await bar(wrapper).trigger('keydown', { key: 'ArrowRight' });
      expect(bar(wrapper).attributes('style')).toContain('grid-column: 3 / 4');
    });

    it('同じ行の帯を追い越しても DOM の並びを変えない（操作中のフォーカスを外さない）', async () => {
      const first = task({
        id: 't-a',
        planned_start_on: '2026-10-13',
        soft_deadline: '2026-10-15T00:00:00Z',
      });
      const second = task({
        id: 't-b',
        planned_start_on: '2026-10-14',
        soft_deadline: '2026-10-16T00:00:00Z',
      });
      const wrapper = mountTimeline([first, second]);
      const order = () =>
        wrapper
          .findAll(`[data-member-row="${ALICE.id}"] [data-task-id]`)
          .map((e) => e.attributes('data-task-id'));
      const before = wrapper.get('[data-task-id="t-b"]').element;
      expect(order()).toEqual(['t-a', 't-b']);
      await wrapper.get('[data-task-id="t-b"]').trigger('keydown', { key: 'ArrowLeft' });
      await wrapper.get('[data-task-id="t-b"]').trigger('keydown', { key: 'ArrowLeft' });
      // t-b の始点が t-a より前になっても要素も順も同じ
      expect(wrapper.get('[data-task-id="t-b"]').attributes('style')).toContain(
        'grid-column: 2 / 7',
      );
      expect(order()).toEqual(['t-a', 't-b']);
      expect(wrapper.get('[data-task-id="t-b"]').element).toBe(before);
    });

    it('始点が範囲より前の帯は、元より外へは動かさず範囲に近づける向きだけ動かす', async () => {
      // 10/8〜10/14 の帯は範囲（10/12〜）の初日から切り詰めて描く
      const wrapper = mountTimeline([
        task({ planned_start_on: '2026-10-08', soft_deadline: '2026-10-14T00:00:00Z' }),
      ]);
      await bar(wrapper).trigger('keydown', { key: 'ArrowLeft' });
      expect(bar(wrapper).attributes('data-moving')).toBeUndefined();
      await bar(wrapper).trigger('keydown', { key: 'ArrowRight' });
      await bar(wrapper).trigger('keydown', { key: 'ArrowLeft' });
      await bar(wrapper).trigger('keydown', { key: 'ArrowLeft' });
      // 戻せるのは元の 10/8 まで
      await bar(wrapper).trigger('keydown', { key: 'Enter' });
      await flushPromises();
      expect(put).not.toHaveBeenCalled();
    });

    it('両端が範囲外の帯でも、範囲から外れる位置には動かさない', async () => {
      // 開始予定日 10/1・締切 11/1 の帯は範囲（10/12〜10/25）いっぱいに描く。
      // 開始予定日を 10/26 にすると帯は 10/26〜11/1 になり、範囲から外れる
      const wrapper = mountTimeline([
        task({ planned_start_on: '2026-10-01', soft_deadline: '2026-11-01T00:00:00Z' }),
      ]);
      for (let i = 0; i < 30; i++) await bar(wrapper).trigger('keydown', { key: 'ArrowRight' });
      expect(bar(wrapper).attributes('style')).toContain('grid-column: 15 / 16');
    });

    it('キーボードで動かしたまま別の帯をドラッグすると、前の移動は捨ててドラッグした帯だけ送る', async () => {
      put.mockResolvedValue({ data: {}, error: undefined, response: { status: 200 } });
      const other = task({
        id: 't-2',
        planned_start_on: '2026-10-19',
        soft_deadline: '2026-10-21T00:00:00Z',
      });
      const wrapper = mountTimeline([movable(), other]);
      await bar(wrapper).trigger('keydown', { key: 'ArrowLeft' });
      const second = wrapper.get(`[data-member-row="${ALICE.id}"] [data-task-id="t-2"]`);
      await second.trigger('pointerdown', { clientX: 0, button: 0 });
      expect(bar(wrapper).attributes('style')).toContain('grid-column: 3 / 6');
      await second.trigger('pointermove', { clientX: -100 });
      await second.trigger('pointerup');
      await flushPromises();
      expect(put).toHaveBeenCalledTimes(1);
      expect(put.mock.calls[0]![1].params.path.id).toBe('t-2');
      expect(put.mock.calls[0]![1].body).toEqual({ planned_start_on: '2026-10-18' });
    });

    it('キーボードで動かした帯をクリックしても確定しない', async () => {
      const wrapper = mountTimeline([movable()]);
      await bar(wrapper).trigger('keydown', { key: 'ArrowLeft' });
      await bar(wrapper).trigger('pointerdown', { clientX: 0, button: 0 });
      await bar(wrapper).trigger('pointerup');
      await flushPromises();
      expect(put).not.toHaveBeenCalled();
      expect(bar(wrapper).attributes('style')).toContain('grid-column: 3 / 6');
    });

    it('ドラッグ中にポインタのキャプチャが外れたら取り消し、後の pointerup で送らない', async () => {
      const wrapper = mountTimeline([movable()]);
      await bar(wrapper).trigger('pointerdown', { clientX: 500, button: 0 });
      await bar(wrapper).trigger('pointermove', { clientX: 400 });
      await bar(wrapper).trigger('lostpointercapture');
      expect(bar(wrapper).attributes('style')).toContain('grid-column: 3 / 6');
      await bar(wrapper).trigger('pointermove', { clientX: 300 });
      await bar(wrapper).trigger('pointerup');
      await flushPromises();
      expect(put).not.toHaveBeenCalled();
    });

    it('キーボードで確定した帯は、取り直しでフォーカスが外れても戻す', async () => {
      put.mockResolvedValue({ data: {}, error: undefined, response: { status: 200 } });
      const wrapper = mountTimeline([movable()], [ALICE, BOB], {}, document.body);
      // 取り直しで要素が動いてフォーカスが外れた状態を作る
      wrapper.invalidate.mockImplementation(async () => {
        (document.activeElement as HTMLElement).blur();
      });
      (bar(wrapper).element as HTMLElement).focus();
      await bar(wrapper).trigger('keydown', { key: 'ArrowLeft' });
      await bar(wrapper).trigger('keydown', { key: 'Enter' });
      await flushPromises();
      expect(document.activeElement).toBe(bar(wrapper).element);
      wrapper.unmount();
    });

    it('保存を待つ間に別の要素へフォーカスを移したら、完了しても奪わない', async () => {
      put.mockResolvedValue({ data: {}, error: undefined, response: { status: 200 } });
      const other = document.createElement('button');
      document.body.append(other);
      const wrapper = mountTimeline([movable()], [ALICE, BOB], {}, document.body);
      wrapper.invalidate.mockImplementation(async () => other.focus());
      (bar(wrapper).element as HTMLElement).focus();
      await bar(wrapper).trigger('keydown', { key: 'ArrowLeft' });
      await bar(wrapper).trigger('keydown', { key: 'Enter' });
      await flushPromises();
      expect(document.activeElement).toBe(other);
      wrapper.unmount();
      other.remove();
    });

    it('取り消したドラッグの後の click は握り潰さない', async () => {
      const wrapper = mountTimeline([movable()]);
      await bar(wrapper).trigger('pointerdown', { clientX: 0, button: 0 });
      await bar(wrapper).trigger('pointermove', { clientX: 300 });
      await bar(wrapper).trigger('pointercancel');
      const click = new MouseEvent('click', { bubbles: true, cancelable: true, button: 0 });
      bar(wrapper).element.dispatchEvent(click);
      expect(click.defaultPrevented).toBe(true);
      const { navigate } = await import('vike/client/router');
      expect(navigate).toHaveBeenCalledWith('/acme/projects/ENG/tasks/ENG-1');
      vi.mocked(navigate).mockClear();
    });

    it('ドラッグした後の click ではタスク詳細へ遷移しない', async () => {
      const wrapper = mountTimeline([movable()]);
      await bar(wrapper).trigger('pointerdown', { clientX: 0, button: 0 });
      await bar(wrapper).trigger('pointermove', { clientX: 300 });
      await bar(wrapper).trigger('pointerup');
      const click = new MouseEvent('click', { bubbles: true, cancelable: true, button: 0 });
      bar(wrapper).element.dispatchEvent(click);
      expect(click.defaultPrevented).toBe(true);
      const { navigate } = await import('vike/client/router');
      expect(navigate).not.toHaveBeenCalled();
    });
  });
});
