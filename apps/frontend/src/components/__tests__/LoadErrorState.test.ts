import { describe, expect, it } from 'vitest';
import { mount } from '@vue/test-utils';
import LoadErrorState from '../LoadErrorState.vue';

describe('LoadErrorState', () => {
  it('既定の文言と再試行ボタンを出し、押すと retry を出す', async () => {
    const wrapper = mount(LoadErrorState);

    expect(wrapper.get('[role="alert"]').text()).toContain('読み込みに失敗しました');
    await wrapper.get('button').trigger('click');
    expect(wrapper.emitted('retry')).toHaveLength(1);
  });

  it('文言を差し替えられる', () => {
    const wrapper = mount(LoadErrorState, { props: { message: 'タスクの読み込みに失敗しました' } });

    expect(wrapper.text()).toContain('タスクの読み込みに失敗しました');
  });
});
