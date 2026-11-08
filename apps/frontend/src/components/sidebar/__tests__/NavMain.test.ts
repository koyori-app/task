import { describe, it, expect, afterEach } from 'vitest';
import { mount, enableAutoUnmount } from '@vue/test-utils';

enableAutoUnmount(afterEach);
import NavMain from '../NavMain.vue';

const items = [
  { title: 'Labels', url: '/tenant/projects/proj/labels', isActive: true },
  { title: 'Tasks', url: '/tenant/projects/proj/tasks', isActive: false },
];

function mountNavMain() {
  return mount(NavMain, { props: { items }, attachTo: document.body });
}

function findLink(wrapper: ReturnType<typeof mountNavMain>, name: string) {
  const link = wrapper.findAll('a').find((a) => a.text() === name);
  if (!link) throw new Error(`link "${name}" not found`);
  return link;
}

describe('NavMain', () => {
  it('各項目をリンクで出す', () => {
    const wrapper = mountNavMain();
    expect(findLink(wrapper, 'Labels').attributes('href')).toBe('/tenant/projects/proj/labels');
    expect(findLink(wrapper, 'Tasks').attributes('href')).toBe('/tenant/projects/proj/tasks');
  });

  it('現在地の項目だけを aria-current="page" で強調する', () => {
    const wrapper = mountNavMain();
    expect(findLink(wrapper, 'Labels').attributes('aria-current')).toBe('page');
    expect(findLink(wrapper, 'Tasks').attributes('aria-current')).toBeUndefined();
  });
});
