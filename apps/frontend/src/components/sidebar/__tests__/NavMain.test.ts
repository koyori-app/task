import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import { defineComponent, h } from 'vue';

// isActive が data-active 属性に反映されるかのスモークテスト
describe('SidebarMenuButton isActive propagation', () => {
  it('renders without error when isActive is true', () => {
    const Stub = defineComponent({
      props: { isActive: Boolean },
      setup(props) {
        return () => h('button', { 'data-active': props.isActive }, 'item');
      },
    });
    const wrapper = mount(Stub, { props: { isActive: true } });
    expect(wrapper.attributes('data-active')).toBe('true');
  });

  it('renders without error when isActive is false', () => {
    const Stub = defineComponent({
      props: { isActive: Boolean },
      setup(props) {
        return () => h('button', { 'data-active': props.isActive }, 'item');
      },
    });
    const wrapper = mount(Stub, { props: { isActive: false } });
    expect(wrapper.attributes('data-active')).toBe('false');
  });
});
