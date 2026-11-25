import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { nextTick } from 'vue';
import { mount, type VueWrapper } from '@vue/test-utils';

import TaskDatesChip from '../TaskDatesChip.vue';

type Key = 'plannedStartOn' | 'softDeadline' | 'hardDeadline';

const DATE_PANEL = '[role="dialog"][aria-label="日付"]';

function mountChip(props: { autoAdvance?: boolean; values?: Partial<Record<Key, string>> } = {}) {
  const wrapper: VueWrapper = mount(TaskDatesChip, {
    props: {
      plannedStartOn: props.values?.plannedStartOn ?? '',
      softDeadline: props.values?.softDeadline ?? '',
      hardDeadline: props.values?.hardDeadline ?? '',
      autoAdvance: props.autoAdvance,
      'onUpdate:plannedStartOn': (v: string) => wrapper.setProps({ plannedStartOn: v }),
      'onUpdate:softDeadline': (v: string) => wrapper.setProps({ softDeadline: v }),
      'onUpdate:hardDeadline': (v: string) => wrapper.setProps({ hardDeadline: v }),
    },
    attachTo: document.body,
  });
  return wrapper;
}

/** チップの 3 つの値（v-model を setProps で回しているので props が今の値） */
const dates = (wrapper: VueWrapper) => wrapper.props() as Record<Key, string>;
const panel = () => document.body.querySelector<HTMLElement>(DATE_PANEL);
const activeRow = () =>
  document.body
    .querySelector('[data-date-field][aria-pressed="true"]')
    ?.getAttribute('data-date-field');

async function settle() {
  await nextTick();
  await nextTick();
}

async function toggle(wrapper: VueWrapper) {
  document.body.querySelector<HTMLElement>('[aria-haspopup="dialog"]')!.click();
  await settle();
}

async function selectRow(key: Key) {
  document.body.querySelector<HTMLElement>(`[data-date-field="${key}"]`)!.click();
  await settle();
}

async function clickDay(date: string) {
  const day = document.body.querySelector<HTMLElement>(`${DATE_PANEL} [data-date="${date}"]`);
  if (!day) throw new Error(`${date} not found in the calendar`);
  day.click();
  await settle();
}

describe('TaskDatesChip の行の自動送り', () => {
  beforeEach(() => {
    // カレンダーは今日の月で開く。選ぶ日が同じ月に出るよう今日を固定する
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(new Date('2026-10-15T12:00:00+09:00'));
  });

  afterEach(() => {
    vi.useRealTimers();
    document.body.innerHTML = '';
  });

  it('開始予定日の日を選ぶと、期限の行へ進み、値は開始予定日にだけ入る', async () => {
    const wrapper = mountChip({ autoAdvance: true });
    await toggle(wrapper);
    await selectRow('plannedStartOn');
    await clickDay('2026-10-19');

    expect(activeRow()).toBe('softDeadline');
    expect(wrapper.props()).toMatchObject({
      plannedStartOn: '2026-10-19',
      softDeadline: '',
      hardDeadline: '',
    });
    wrapper.unmount();
  });

  it('続けて期限・最終期限を選ぶと、最終期限の行で止まり、パネルは開いたまま', async () => {
    const wrapper = mountChip({ autoAdvance: true });
    await toggle(wrapper);
    await selectRow('plannedStartOn');
    await clickDay('2026-10-19');
    await clickDay('2026-10-23');

    expect(activeRow()).toBe('hardDeadline');
    expect(dates(wrapper).softDeadline).toBe('2026-10-23');

    await clickDay('2026-10-31');

    expect(activeRow()).toBe('hardDeadline');
    expect(panel()).not.toBeNull();
    expect(wrapper.props()).toMatchObject({
      plannedStartOn: '2026-10-19',
      softDeadline: '2026-10-23',
      hardDeadline: '2026-10-31',
    });
    wrapper.unmount();
  });

  it('開いた直後の期限の行で日を選んでも、行は期限のまま', async () => {
    const wrapper = mountChip({ autoAdvance: true });
    await toggle(wrapper);
    expect(activeRow()).toBe('softDeadline');
    await clickDay('2026-10-23');

    expect(activeRow()).toBe('softDeadline');
    expect(dates(wrapper).softDeadline).toBe('2026-10-23');
    wrapper.unmount();
  });

  it('開始予定日の後に期限の行を手で押すと、連鎖が切れて期限で止まる', async () => {
    const wrapper = mountChip({ autoAdvance: true });
    await toggle(wrapper);
    await selectRow('plannedStartOn');
    await clickDay('2026-10-19');
    await selectRow('softDeadline');
    await clickDay('2026-10-23');

    expect(activeRow()).toBe('softDeadline');
    wrapper.unmount();
  });

  it('開始予定日を選んで閉じ、開き直して期限を選んでも、行は期限のまま', async () => {
    const wrapper = mountChip({ autoAdvance: true });
    await toggle(wrapper);
    await selectRow('plannedStartOn');
    await clickDay('2026-10-19');
    await toggle(wrapper);
    expect(panel()).toBeNull();
    await toggle(wrapper);
    await clickDay('2026-10-23');

    expect(activeRow()).toBe('softDeadline');
    wrapper.unmount();
  });

  it('行が進んだ後もフォーカスはパネルの中に残り、Esc はパネルだけを閉じる', async () => {
    const wrapper = mountChip({ autoAdvance: true });
    await toggle(wrapper);
    await selectRow('plannedStartOn');
    await clickDay('2026-10-19');

    const focused = document.activeElement as HTMLElement;
    expect(panel()!.contains(focused)).toBe(true);
    expect(focused.closest('[role="grid"]')).not.toBeNull();

    // Vue は、先に別の Vue の listener（カレンダーの keydown）を通ったイベントを、時刻が listener を付けた
    // 時刻以前なら捨てる。Date を止めているとパネルの Esc が捨てられるので、時計を進めてから送る
    vi.setSystemTime(new Date().getTime() + 1000);
    const event = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });
    focused.dispatchEvent(event);
    await settle();

    expect(event.defaultPrevented).toBe(true);
    expect(panel()).toBeNull();
    wrapper.unmount();
  });

  it('Enter で選んでも行が進む', async () => {
    const wrapper = mountChip({ autoAdvance: true });
    await toggle(wrapper);
    await selectRow('plannedStartOn');
    await clickDay('2026-10-19');

    const cell = document.activeElement as HTMLElement;
    cell.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }),
    );
    await settle();

    expect(activeRow()).toBe('hardDeadline');
    expect(dates(wrapper).softDeadline).not.toBe('');
    wrapper.unmount();
  });

  it('進めた先に値があっても進み、選び直すと上書きする', async () => {
    const wrapper = mountChip({
      autoAdvance: true,
      values: { softDeadline: '2026-10-20', hardDeadline: '2026-10-28' },
    });
    await toggle(wrapper);
    await selectRow('plannedStartOn');
    await clickDay('2026-10-19');
    expect(activeRow()).toBe('softDeadline');
    await clickDay('2026-10-23');
    expect(activeRow()).toBe('hardDeadline');
    await clickDay('2026-10-31');

    expect(wrapper.props()).toMatchObject({
      plannedStartOn: '2026-10-19',
      softDeadline: '2026-10-23',
      hardDeadline: '2026-10-31',
    });
    wrapper.unmount();
  });

  it('autoAdvance が無ければ、開始予定日の日を選んでも行は動かない', async () => {
    const wrapper = mountChip();
    await toggle(wrapper);
    await selectRow('plannedStartOn');
    await clickDay('2026-10-19');

    expect(activeRow()).toBe('plannedStartOn');
    expect(dates(wrapper).plannedStartOn).toBe('2026-10-19');
    wrapper.unmount();
  });
});
