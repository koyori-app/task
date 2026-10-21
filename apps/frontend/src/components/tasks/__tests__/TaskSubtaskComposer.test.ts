import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';

import TaskSubtaskComposer from '@/components/tasks/TaskSubtaskComposer.vue';

enableAutoUnmount(afterEach);

describe('TaskSubtaskComposer', () => {
  it('Enter で前後空白を除いたタイトルを作成し、成功時だけ入力を消す', async () => {
    const onCreate = vi.fn(async () => true);
    const wrapper = mount(TaskSubtaskComposer, { props: { onCreate } });
    const input = wrapper.get<HTMLInputElement>('input[aria-label="サブタスク名"]');
    await input.setValue('  子タスク  ');
    await input.trigger('keydown', { key: 'Enter' });
    await flushPromises();

    expect(onCreate).toHaveBeenCalledWith('子タスク');
    expect(input.element.value).toBe('');
  });

  it('IME の変換を確定・取り消す Enter / Esc では作成もキャンセルもしない', async () => {
    const onCreate = vi.fn(async () => true);
    const wrapper = mount(TaskSubtaskComposer, { props: { onCreate } });
    const input = wrapper.get<HTMLInputElement>('input[aria-label="サブタスク名"]');
    await input.setValue('変換中');
    await input.trigger('keydown', { key: 'Enter', isComposing: true });
    // Safari は compositionend の後に isComposing=false・keyCode 229 で流してくる
    await input.trigger('keydown', { key: 'Enter', keyCode: 229 });
    await input.trigger('keydown', { key: 'Escape', isComposing: true });
    await flushPromises();

    expect(onCreate).not.toHaveBeenCalled();
    expect(wrapper.emitted('cancel')).toBeUndefined();
    expect(input.element.value).toBe('変換中');
  });

  it('作成失敗時は下書きとエラーを残す', async () => {
    const wrapper = mount(TaskSubtaskComposer, {
      props: {
        onCreate: vi.fn(async () => false),
        error: 'サブタスクを作成できませんでした',
      },
    });
    const input = wrapper.get<HTMLInputElement>('input[aria-label="サブタスク名"]');
    await input.setValue('消さない');
    await input.trigger('keydown', { key: 'Enter' });
    await flushPromises();

    expect(input.element.value).toBe('消さない');
    expect(wrapper.get('[role="alert"]').text()).toBe('サブタスクを作成できませんでした');
  });

  it('disabled 中は UI とハンドラの両方で作成を止める', async () => {
    const onCreate = vi.fn(async () => true);
    const wrapper = mount(TaskSubtaskComposer, { props: { onCreate, disabled: true } });
    const input = wrapper.get<HTMLInputElement>('input[aria-label="サブタスク名"]');

    expect(input.attributes('disabled')).toBeDefined();
    expect(wrapper.get('button:not([aria-label])').attributes('disabled')).toBeDefined();
    await input.trigger('keydown', { key: 'Enter' });

    expect(onCreate).not.toHaveBeenCalled();
  });

  it('Escape で追加を取り消す', async () => {
    const wrapper = mount(TaskSubtaskComposer, {
      props: { onCreate: vi.fn(async () => true) },
    });
    await wrapper.get('input').trigger('keydown', { key: 'Escape' });
    expect(wrapper.emitted('cancel')).toHaveLength(1);
  });
});
