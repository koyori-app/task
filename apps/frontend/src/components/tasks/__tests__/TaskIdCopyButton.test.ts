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

  // TASK-524: スマホの狭い 2 段目で、番号の桁とアイコンの幅を取って期限の上へはみ出さない
  it('compact は幅を先に取らず、親より広がらずに省略する', async () => {
    const writeText = vi.fn(async () => {});
    stubClipboard(writeText);
    const wrapper = mount(TaskIdCopyButton, {
      props: { projectKey: 'ABCDEFGH', seqId: 1, compact: true },
    });

    expect(wrapper.classes()).toEqual(expect.arrayContaining(['min-w-0', 'max-w-full']));
    expect(wrapper.classes()).not.toContain('shrink-0');
    const text = wrapper.get('span');
    expect(text.attributes('style')).toBeUndefined();
    expect(text.classes()).toContain('truncate');
    expect(wrapper.find('svg').exists()).toBe(false);

    // 押した結果は出す
    await wrapper.trigger('click');
    await flushPromises();
    expect(writeText).toHaveBeenCalledWith('ABCDEFGH-1');
    expect(wrapper.find('svg').exists()).toBe(true);
  });
});
