import { afterEach, describe, expect, it } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';

import UserAvatar from '@/components/UserAvatar.vue';

enableAutoUnmount(afterEach);

/** 頭文字は koyori の Avatar が `role="img"` の要素に出し、名で読み上げる。 */
const INITIALS = '[role=img]';

describe('UserAvatar の頭文字', () => {
  it('画像が無ければ頭文字を出し、名で読み上げる', () => {
    const wrapper = mount(UserAvatar, { props: { user: { username: 'yupix' } } });

    expect(wrapper.get(INITIALS).attributes('aria-label')).toBe('yupix');
    expect(wrapper.text()).toBe('y');
    expect(wrapper.find('img').exists()).toBe(false);
  });

  it('name があれば username より先に使う', () => {
    const wrapper = mount(UserAvatar, {
      props: { user: { name: 'あきら', username: 'akira' } },
    });

    expect(wrapper.get(INITIALS).attributes('aria-label')).toBe('あきら');
  });

  it('avatar_url が空文字・null でも頭文字を出す', () => {
    for (const avatar_url of ['', null]) {
      const wrapper = mount(UserAvatar, { props: { user: { username: 'sousuke', avatar_url } } });
      expect(wrapper.get(INITIALS).attributes('aria-label')).toBe('sousuke');
      expect(wrapper.find('img').exists()).toBe(false);
    }
  });
});

describe('UserAvatar の画像', () => {
  it('画像を名の alt で出し、頭文字は出さない', () => {
    const wrapper = mount(UserAvatar, {
      props: { user: { username: 'yupix', avatar_url: 'https://example.test/a.png' } },
    });

    const img = wrapper.get('img');
    expect(img.attributes('src')).toBe('https://example.test/a.png');
    expect(img.attributes('alt')).toBe('yupix');
    expect(wrapper.find(INITIALS).exists()).toBe(false);
  });

  it('decorative なら読み上げから外す（隣に名が出ている時）。通常は外さない', () => {
    const user = { username: 'yupix', avatar_url: 'https://example.test/a.png' };

    expect(mount(UserAvatar, { props: { user } }).attributes('aria-hidden')).toBeUndefined();
    expect(mount(UserAvatar, { props: { user, decorative: true } }).attributes('aria-hidden')).toBe(
      'true',
    );
  });

  it('画像の読み込みに失敗したら頭文字に落ちる', async () => {
    const wrapper = mount(UserAvatar, {
      props: { user: { username: 'yupix', avatar_url: 'https://example.test/broken.png' } },
    });
    await wrapper.get('img').trigger('error');

    expect(wrapper.find('img').exists()).toBe(false);
    expect(wrapper.get(INITIALS).attributes('aria-label')).toBe('yupix');
  });
});

describe('UserAvatar の大きさ・形・灰色', () => {
  it('既定は 32px の丸', () => {
    const wrapper = mount(UserAvatar, { props: { user: { username: 'a' } } });

    expect(wrapper.element.tagName).toBe('SPAN');
    expect(wrapper.attributes('style')).toContain('width: 32px');
    expect(wrapper.classes()).toContain('rounded-full');
  });

  it.each([
    ['xs', 16],
    ['sm', 20],
    ['md', 24],
    ['lg', 28],
    ['xl', 32],
    ['2xl', 56],
    ['3xl', 64],
  ] as const)('size=%s は直径 %ipx', (size, px) => {
    const style = mount(UserAvatar, { props: { user: { username: 'a' }, size } }).attributes(
      'style',
    );

    expect(style).toContain(`width: ${px}px`);
    expect(style).toContain(`height: ${px}px`);
  });

  it('shape で角の形を替える', () => {
    const wrapper = mount(UserAvatar, { props: { user: { username: 'a' }, shape: 'lg' } });

    expect(wrapper.classes()).toContain('rounded-lg');
    expect(wrapper.classes()).not.toContain('rounded-full');
  });

  it('inactive は灰色（薄く）にする。通常は付けない', () => {
    const active = mount(UserAvatar, { props: { user: { username: 'a' } } });
    const inactive = mount(UserAvatar, { props: { user: { username: 'a' }, inactive: true } });

    expect(active.classes()).not.toContain('grayscale');
    expect(inactive.classes()).toEqual(expect.arrayContaining(['grayscale', 'opacity-75']));
  });

  it('class は根に足され、aria-hidden・style 等の属性は根へ渡る', () => {
    const wrapper = mount(UserAvatar, {
      props: { user: { username: 'a' }, class: 'ring-2' },
      attrs: { 'aria-hidden': 'true', style: 'background: #0f766e' },
    });

    expect(wrapper.classes()).toContain('ring-2');
    expect(wrapper.attributes('aria-hidden')).toBe('true');
    expect(wrapper.attributes('style')).toContain('background: #0f766e');
  });

  it('頭文字の色は bg-muted 上の text-foreground の一組', () => {
    const wrapper = mount(UserAvatar, { props: { user: { username: 'a' } } });

    expect(wrapper.classes()).toEqual(expect.arrayContaining(['bg-muted', 'text-foreground']));
    expect(wrapper.classes()).not.toContain('text-muted-foreground');
  });

  it('class の text-white は既定の字の色に勝つ（色つきの地に載せる所）', () => {
    const wrapper = mount(UserAvatar, {
      props: { user: { username: 'a' }, class: 'text-white' },
    });

    expect(wrapper.classes()).toContain('text-white');
    expect(wrapper.classes()).not.toContain('text-foreground');
  });
});
