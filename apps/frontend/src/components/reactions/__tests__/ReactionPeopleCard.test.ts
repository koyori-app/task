import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';

import type { ReactionUser } from '@/lib/reactions';

import ReactionPeopleCard from '../ReactionPeopleCard.vue';

function users(count: number): ReactionUser[] {
  return Array.from({ length: count }, (_, index) => ({
    id: `user-${index}`,
    name: `利用者 ${index}`,
    avatar_url: null,
  }));
}

describe('ReactionPeopleCard', () => {
  it('5 人で切れて「すべて見る」が出る', async () => {
    const wrapper = mount(ReactionPeopleCard, {
      props: { emoji: '👍', count: 8, users: users(8) },
    });
    expect(wrapper.findAll('li')).toHaveLength(5);
    const button = wrapper.get('button');
    expect(button.text()).toBe('すべて見る（8 人）');
    await button.trigger('click');
    expect(wrapper.emitted('showAll')).toHaveLength(1);
  });

  it('5 人までなら「すべて見る」を出さない', () => {
    const wrapper = mount(ReactionPeopleCard, {
      props: { emoji: '👍', count: 5, users: users(5) },
    });
    expect(wrapper.findAll('li')).toHaveLength(5);
    expect(wrapper.find('button').exists()).toBe(false);
  });

  it('自分には「自分」と添え、長い名前は title に全文を持つ', () => {
    const wrapper = mount(ReactionPeopleCard, {
      props: { emoji: '👍', count: 2, users: users(2), currentUserId: 'user-1' },
    });
    const [other, me] = wrapper.findAll('li');
    expect(other?.text()).not.toContain('自分');
    expect(me?.text()).toContain('自分');
    expect(me?.find('[title="利用者 1"]').exists()).toBe(true);
  });
});
