import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';

import LabelMark from '../LabelMark.vue';

function dot(wrapper: ReturnType<typeof mount>) {
  return wrapper.find('.rounded-full');
}

describe('LabelMark', () => {
  it('絵文字を持つラベルは絵文字を出し、色の丸を出さない', () => {
    const wrapper = mount(LabelMark, {
      props: { label: { color: '#e11d48', icon_emoji: '🫟' } },
    });
    expect(wrapper.text()).toBe('🫟');
    expect(wrapper.attributes('aria-hidden')).toBe('true');
    expect(dot(wrapper).exists()).toBe(false);
  });

  it.each([
    ['無い', {}],
    ['null', { icon_emoji: null }],
    ['空文字', { icon_emoji: '' }],
  ])('絵文字が%sラベルは色の丸を出す', (_, extra) => {
    const wrapper = mount(LabelMark, { props: { label: { color: '#e11d48', ...extra } } });
    expect(wrapper.text()).toBe('');
    expect(dot(wrapper).exists()).toBe(true);
    expect(dot(wrapper).attributes('style')).toContain('background-color');
    expect(dot(wrapper).attributes('aria-hidden')).toBe('true');
  });
});
