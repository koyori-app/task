import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';

import TaskIdCopyButton from '../TaskIdCopyButton.vue';

enableAutoUnmount(afterEach);

function stubClipboard(writeText: (text: string) => Promise<void>) {
  Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
}

function mountButton() {
  return mount(TaskIdCopyButton, { props: { projectKey: 'TEST', seqId: 42 } });
}

describe('TaskIdCopyButton', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    delete (navigator as { clipboard?: unknown }).clipboard;
  });

  it('ID を出し、押すと ID だけをコピーして 2 秒で戻る', async () => {
    const writeText = vi.fn(async () => {});
    stubClipboard(writeText);
    const wrapper = mountButton();

    expect(wrapper.text()).toBe('TEST-42');
    expect(wrapper.attributes('aria-label')).toBe('タスクID TEST-42 をコピー');

    await wrapper.trigger('click');
    await flushPromises();

    expect(writeText).toHaveBeenCalledWith('TEST-42');
    expect(wrapper.attributes('aria-label')).toBe('タスクIDをコピーしました');

    vi.advanceTimersByTime(2000);
    await flushPromises();
    expect(wrapper.attributes('aria-label')).toBe('タスクID TEST-42 をコピー');
  });

  it('コピーに失敗したら握り潰さず失敗を示す', async () => {
    stubClipboard(vi.fn(async () => Promise.reject(new Error('denied'))));
    const wrapper = mountButton();

    await wrapper.trigger('click');
    await flushPromises();

    expect(wrapper.attributes('aria-label')).toBe('タスクIDをコピーできませんでした');
    expect(wrapper.classes()).toContain('text-destructive');
  });

  // 桁数で幅が変わると、後ろに並ぶラベルの開始位置が行ごとにずれる
  it('文字の幅はプロジェクトキーの長さで決まり、番号の桁数で変わらない', () => {
    const short = mount(TaskIdCopyButton, { props: { projectKey: 'TEST', seqId: 1 } });
    const long = mount(TaskIdCopyButton, { props: { projectKey: 'TEST', seqId: 12345 } });

    expect(short.get('span').attributes('style')).toContain('width: 10ch');
    expect(long.get('span').attributes('style')).toBe(short.get('span').attributes('style'));
  });

  // Safari はボタンを押してもフォーカスせず外側の行へ移し、行が選択されてしまう
  it('押してもフォーカスを動かさない', async () => {
    const wrapper = mountButton();
    const down = new MouseEvent('mousedown', { bubbles: true, cancelable: true });

    wrapper.element.dispatchEvent(down);

    expect(down.defaultPrevented).toBe(true);
  });

  it('コピーのアイコンは場所を取ったまま隠し、行のホバーとフォーカスで出す', () => {
    const icon = mountButton().get('svg');

    expect(icon.classes()).toContain('opacity-0');
    expect(icon.classes()).toContain('group-hover:opacity-100');
    expect(icon.classes()).toContain('group-focus-within:opacity-100');
  });

  // TASK-524: 狭い 2 段目では番号を欠かさず、入らないときはキーの側だけを省略する
  it('compact はキーと番号を分け、番号は縮めず、根は縮まずに親より広がらない', () => {
    const wrapper = mount(TaskIdCopyButton, {
      props: { projectKey: 'ABCDEFGHIJ', seqId: 12345, compact: true },
    });

    expect(wrapper.classes()).toEqual(expect.arrayContaining(['shrink-0', 'max-w-full']));
    expect(wrapper.classes()).not.toContain('min-w-0');
    // 親の overflow-hidden に切られないよう、左へはみ出さない
    expect(wrapper.classes()).not.toContain('-ml-1.5');
    const key = wrapper.get('[data-task-id-key]');
    const number = wrapper.get('[data-task-id-number]');
    expect(key.text()).toBe('ABCDEFGHIJ');
    expect(key.classes()).toEqual(expect.arrayContaining(['min-w-0', 'truncate']));
    expect(number.text()).toBe('-12345');
    expect(number.classes()).toContain('shrink-0');
    // 表示が省略されても、名前と title は ID の全体
    expect(wrapper.attributes('aria-label')).toBe('タスクID ABCDEFGHIJ-12345 をコピー');
    expect(wrapper.attributes('title')).toBe('ABCDEFGHIJ-12345');
    expect(wrapper.find('svg').exists()).toBe(false);
  });

  it.each([
    ['成功', async () => {}, 'タスクIDをコピーしました'],
    ['失敗', async () => Promise.reject(new Error('denied')), 'タスクIDをコピーできませんでした'],
  ])(
    'compact はコピーの%sでもアイコンを足さず、色と読み上げの名前で伝える',
    async (_, write, name) => {
      const writeText = vi.fn(write);
      stubClipboard(writeText);
      const wrapper = mount(TaskIdCopyButton, {
        props: { projectKey: 'TASK', seqId: 123, compact: true },
      });

      await wrapper.trigger('click');
      await flushPromises();

      expect(writeText).toHaveBeenCalledWith('TASK-123');
      expect(wrapper.attributes('aria-label')).toBe(name);
      expect(wrapper.find('svg').exists()).toBe(false);
      expect(wrapper.text()).toBe('TASK-123');
    },
  );

  it('対照: compact でなければ幅を先に取り、アイコンの場所を持つ', () => {
    const wrapper = mountButton();

    expect(wrapper.classes()).toEqual(expect.arrayContaining(['shrink-0', '-ml-1.5']));
    expect(wrapper.get('span').attributes('style')).toContain('width');
    expect(wrapper.find('svg').exists()).toBe(true);
    expect(wrapper.attributes('title')).toBeUndefined();
  });
});
