import { describe, expect, it } from 'vitest';
import { mount } from '@vue/test-utils';

import type { components } from '@/generated/api';

import TaskIssueSourceRow from '../TaskIssueSourceRow.vue';

const issue: components['schemas']['TaskForgeIssue'] = {
  host: 'github',
  repo_owner: 'acme',
  repo_name: 'backend',
  number: 42,
  html_url: 'https://github.com/acme/backend/issues/42',
  state: 'open',
  last_synced_at: '2026-10-01T00:00:00Z',
  pending_push: false,
};

function mountWith(overrides: Partial<typeof issue> = {}) {
  return mount(TaskIssueSourceRow, { props: { issue: { ...issue, ...overrides } } });
}

describe('TaskIssueSourceRow', () => {
  it('owner/repo#N を Issue へのリンクにし、状態と最終同期を出す', () => {
    const wrapper = mountWith();
    const link = wrapper.get('a');
    expect(link.text()).toBe('acme/backend#42');
    expect(link.attributes('href')).toBe(issue.html_url);
    expect(link.attributes('rel')).toBe('noopener noreferrer');
    expect(wrapper.text()).toContain('取り込み元');
    expect(wrapper.text()).toContain('Open');
    expect(wrapper.text()).toContain('最終同期');
    expect(wrapper.text()).not.toContain('GitHub への反映待ち');
  });

  it.each([
    ['closed', 'Closed'],
    ['unknown', '状態不明'],
  ])('状態 %s は %s と出す', (state, label) => {
    expect(mountWith({ state }).text()).toContain(label);
  });

  it('同期の時刻を控える前の行（last_synced_at が null）では最終同期を出さない', () => {
    expect(mountWith({ last_synced_at: null }).text()).not.toContain('最終同期');
  });

  it('pending_push なら反映待ちを出す', () => {
    expect(mountWith({ pending_push: true }).text()).toContain('GitHub への反映待ち');
  });

  it('http(s) 以外の URL はリンクにしない', () => {
    const wrapper = mountWith({ html_url: 'javascript:alert(1)' });
    expect(wrapper.find('a').exists()).toBe(false);
    expect(wrapper.text()).toContain('acme/backend#42');
  });
});
