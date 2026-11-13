import { afterEach, describe, expect, it } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';

import MentionProfileCard from '../MentionProfileCard.vue';

enableAutoUnmount(afterEach);

const member = {
  kind: 'member' as const,
  name: 'hanako',
  avatarUrl: null,
  meta: 'デザイナー · プロダクト部 · UI チーム',
  bio: '画面と部品のデザインを担当しています。',
  href: '/acme/users/11111111-1111-4111-8111-111111111111',
};

describe('MentionProfileCard', () => {
  it('メンバー: 名前・所属・自己紹介・プロフィールへのリンク・Esc の案内', () => {
    const wrapper = mount(MentionProfileCard, { props: member });
    expect(wrapper.get('.name').text()).toBe('hanako');
    expect(wrapper.get('.meta').text()).toBe('デザイナー · プロダクト部 · UI チーム');
    expect(wrapper.get('.bio').text()).toBe('画面と部品のデザインを担当しています。');
    const link = wrapper.get('.foot a');
    expect(link.attributes('href')).toBe(member.href);
    expect(link.text()).toContain('プロフィールを見る');
    expect(wrapper.get('.hint').text()).toBe('Esc で閉じる');
    expect(wrapper.find('.badge').exists()).toBe(false);
    expect(wrapper.find('.notice').exists()).toBe(false);
  });

  it('本人は「あなた」、ゲストは「ゲスト」の印', () => {
    const self = mount(MentionProfileCard, { props: { ...member, self: true } });
    expect(self.get('.badge').text()).toBe('あなた');
    expect(self.get('.badge').classes()).toContain('is-self');
    const guest = mount(MentionProfileCard, { props: { ...member, guest: true } });
    expect(guest.get('.badge').text()).toBe('ゲスト');
    expect(guest.get('.badge').classes()).not.toContain('is-self');
  });

  it('meta が無ければ .meta を、bio が無ければ .body を出さない', () => {
    const wrapper = mount(MentionProfileCard, { props: { ...member, meta: null, bio: null } });
    expect(wrapper.find('.meta').exists()).toBe(false);
    expect(wrapper.find('.body').exists()).toBe(false);
    expect(wrapper.find('.foot').exists()).toBe(true);
  });

  it('外れた者: 注記を出し、アバターを灰色にする', () => {
    const wrapper = mount(MentionProfileCard, { props: { ...member, kind: 'outside' } });
    expect(wrapper.get('.notice').text()).toContain(
      'このプロジェクトのメンバーではありません。メンションしても通知は届きません。',
    );
    expect(wrapper.get('.notice svg').attributes('aria-hidden')).toBe('true');
    expect(wrapper.get('.head .is-inactive').classes()).toEqual(
      expect.arrayContaining(['opacity-75', 'grayscale']),
    );
  });

  it('メンバーのアバターは灰色にしない', () => {
    const wrapper = mount(MentionProfileCard, { props: member });
    expect(wrapper.find('.is-inactive').exists()).toBe(false);
  });

  it('退会した者: 固定の名前と注記で、下段もリンクも無い', () => {
    const wrapper = mount(MentionProfileCard, {
      props: { kind: 'gone', name: '使われない', href: '/acme/users/x', meta: 'x', bio: 'y' },
    });
    expect(wrapper.get('.name').text()).toBe('退会した利用者');
    expect(wrapper.get('.notice').text()).toContain('この利用者はテナントから退会しています。');
    expect(wrapper.find('.foot').exists()).toBe(false);
    expect(wrapper.find('a').exists()).toBe(false);
    expect(wrapper.find('.bio').exists()).toBe(false);
    expect(wrapper.find('.meta').exists()).toBe(false);
    expect(wrapper.find('.is-inactive').exists()).toBe(true);
  });

  it('href が無ければ下段ごと出さない', () => {
    const wrapper = mount(MentionProfileCard, { props: { ...member, href: null } });
    expect(wrapper.find('.foot').exists()).toBe(false);
  });

  it('アバターは読み上げから外す（名前を隣に字で出すため）', () => {
    const wrapper = mount(MentionProfileCard, { props: member });
    expect(wrapper.get('.head > :first-child').attributes('aria-hidden')).toBe('true');
  });
});
