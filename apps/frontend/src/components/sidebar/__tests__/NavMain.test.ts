import { describe, it, expect, afterEach } from 'vitest';
import { mount, enableAutoUnmount } from '@vue/test-utils';

enableAutoUnmount(afterEach);
import NavMain from '../NavMain.vue';
import SidebarProvider from '../../ui/sidebar/SidebarProvider.vue';

const leafItems = [
  { title: 'Labels', url: '/tenant/projects/proj/labels', isActive: true },
  { title: 'Tasks', url: '/tenant/projects/proj/tasks', isActive: false },
];

// isActive: true を設定すると Collapsible が default-open になりコンテンツが DOM に現れる
const treeItems = [
  { title: 'Settings', url: '/settings', isActive: true, items: [
    { title: 'General', url: '/settings/general' },
    { title: 'Team', url: '/settings/team' },
  ]},
];

function mountNavMain(items: typeof leafItems | typeof treeItems) {
  return mount(
    {
      components: { SidebarProvider, NavMain },
      template: '<SidebarProvider><NavMain :items="items" /></SidebarProvider>',
      data: () => ({ items }),
    },
    { attachTo: document.body },
  );
}

describe('NavMain – leaf items (no sub-items)', () => {
  it('renders each leaf item as a direct <a> link', () => {
    const wrapper = mountNavMain(leafItems);
    const links = wrapper.findAll('a');
    const hrefs = links.map(l => l.attributes('href'));
    expect(hrefs).toContain('/tenant/projects/proj/labels');
    expect(hrefs).toContain('/tenant/projects/proj/tasks');
  });

  it('renders item titles', () => {
    const wrapper = mountNavMain(leafItems);
    expect(wrapper.text()).toContain('Labels');
    expect(wrapper.text()).toContain('Tasks');
  });

  it('active leaf item has data-active attribute set to true', () => {
    const wrapper = mountNavMain(leafItems);
    // SidebarMenuButtonChild renders with data-active on the element (via reka-ui Primitive as-child → merged onto <a>)
    const activeLinks = wrapper.findAll('[data-active="true"]');
    expect(activeLinks.length).toBeGreaterThan(0);
    expect(activeLinks[0].text()).toContain('Labels');
  });

  it('inactive leaf item has data-active attribute set to false', () => {
    const wrapper = mountNavMain(leafItems);
    const inactiveLinks = wrapper.findAll('[data-active="false"]');
    expect(inactiveLinks.length).toBeGreaterThan(0);
    expect(inactiveLinks[0].text()).toContain('Tasks');
  });
});

describe('NavMain – collapsible items (with sub-items)', () => {
  it('renders the parent item title', () => {
    const wrapper = mountNavMain(treeItems);
    expect(wrapper.text()).toContain('Settings');
  });

  it('renders sub-item links when collapsible is open (isActive: true)', () => {
    const wrapper = mountNavMain(treeItems);
    const links = wrapper.findAll('a');
    const hrefs = links.map(l => l.attributes('href'));
    expect(hrefs).toContain('/settings/general');
    expect(hrefs).toContain('/settings/team');
  });

  it('renders sub-item titles when collapsible is open (isActive: true)', () => {
    const wrapper = mountNavMain(treeItems);
    expect(wrapper.text()).toContain('General');
    expect(wrapper.text()).toContain('Team');
  });
});
