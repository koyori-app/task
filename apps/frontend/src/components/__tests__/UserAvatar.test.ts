import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';

import UserAvatar from '@/components/UserAvatar.vue';

enableAutoUnmount(afterEach);

/**
 * reka-ui の AvatarImage は `new Image()` の load / error を見て、読めた時だけ <img> を出す。
 * テスト環境は画像を読まぬので、読み込みの成否を決める Image に差し替える。
 */
function stubImage(result: 'load' | 'error') {
  class FakeImage extends EventTarget {
    complete = false;
    naturalWidth = 0;
    referrerPolicy = '';
    crossOrigin: string | null = null;
    #src = '';
    get src() {
      return this.#src;
    }
    set src(value: string) {
      this.#src = value;
      queueMicrotask(() => this.dispatchEvent(new Event(result)));
    }
  }
  vi.stubGlobal('Image', FakeImage);
}

beforeEach(() => stubImage('load'));
afterEach(() => vi.unstubAllGlobals());

describe('UserAvatar の頭文字', () => {
  it('画像が無ければ名の先頭 2 文字を大文字で出す', () => {
    const wrapper = mount(UserAvatar, { props: { user: { username: 'yupix' } } });

    expect(wrapper.text()).toBe('YU');
    expect(wrapper.find('img').exists()).toBe(false);
  });

  it('name があれば username より先に使う', () => {
    const wrapper = mount(UserAvatar, {
      props: { user: { name: 'あきら', username: 'akira' } },
    });

    expect(wrapper.text()).toBe('あき');
  });

  it('サロゲートペアの字を途中で割らない', () => {
    const wrapper = mount(UserAvatar, { props: { user: { name: '😀😀x' } } });

    expect(wrapper.text()).toBe('😀😀');
  });

  it('initialLength で字数を替えられる', () => {
    const wrapper = mount(UserAvatar, {
      props: { user: { username: 'yupix' }, initialLength: 1 },
    });

    expect(wrapper.text()).toBe('Y');
  });

  it('fallbackText があれば名から作らずその字を出す', () => {
    const wrapper = mount(UserAvatar, {
      props: { user: { name: '' }, fallbackText: '+3' },
    });

    expect(wrapper.text()).toBe('+3');
  });

  it('avatar_url が空文字・null でも頭文字を出す', () => {
    for (const avatar_url of ['', null]) {
      const wrapper = mount(UserAvatar, { props: { user: { username: 'sousuke', avatar_url } } });
      expect(wrapper.text()).toBe('SO');
      expect(wrapper.find('img').exists()).toBe(false);
    }
  });
});

describe('UserAvatar の画像', () => {
  it('読めた画像を名の alt で出す', async () => {
    const wrapper = mount(UserAvatar, {
      props: { user: { username: 'yupix', avatar_url: 'https://example.test/a.png' } },
    });
    await flushPromises();

    const img = wrapper.find('img');
    expect(img.attributes('src')).toBe('https://example.test/a.png');
    expect(img.attributes('alt')).toBe('yupix');
    // 読めた画像が出ている間は頭文字を出さない
    expect(img.attributes('style') ?? '').not.toContain('display: none');
    expect(wrapper.text()).toBe('');
  });

  it('decorative なら alt を空にする（隣に名が出ている時）', async () => {
    const wrapper = mount(UserAvatar, {
      props: {
        user: { username: 'yupix', avatar_url: 'https://example.test/a.png' },
        decorative: true,
      },
    });
    await flushPromises();

    expect(wrapper.find('img').attributes('alt')).toBe('');
  });

  it('画像の読み込みに失敗したら頭文字に落ちる', async () => {
    stubImage('error');
    const wrapper = mount(UserAvatar, {
      props: { user: { username: 'yupix', avatar_url: 'https://example.test/broken.png' } },
    });
    await flushPromises();

    expect(wrapper.find('img').attributes('style') ?? '').toContain('display: none');
    expect(wrapper.text()).toBe('YU');
  });
});

describe('UserAvatar の大きさ・形・灰色', () => {
  it('既定は size-8 の丸', () => {
    const wrapper = mount(UserAvatar, { props: { user: { username: 'a' } } });

    expect(wrapper.classes()).toEqual(expect.arrayContaining(['size-8', 'rounded-full']));
  });

  it.each([
    ['xs', 'size-4'],
    ['sm', 'size-5'],
    ['md', 'size-6'],
    ['lg', 'size-7'],
    ['xl', 'size-8'],
    ['2xl', 'size-14'],
    ['3xl', 'size-16'],
  ] as const)('size=%s は %s', (size, expected) => {
    const wrapper = mount(UserAvatar, { props: { user: { username: 'a' }, size } });

    expect(wrapper.classes()).toContain(expected);
    // 既定の size-8 が残って二重に効かない
    if (expected !== 'size-8') expect(wrapper.classes()).not.toContain('size-8');
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
    expect(inactive.classes()).toEqual(expect.arrayContaining(['grayscale', 'opacity-60']));
  });

  it('class は根に足され、aria-hidden 等の属性は根へ渡る', () => {
    const wrapper = mount(UserAvatar, {
      props: { user: { username: 'a' }, class: 'ring-2' },
      attrs: { 'aria-hidden': 'true' },
    });

    expect(wrapper.classes()).toContain('ring-2');
    expect(wrapper.attributes('aria-hidden')).toBe('true');
  });

  it('tone で頭文字の色を替える', () => {
    const muted = mount(UserAvatar, { props: { user: { username: 'a' } } });
    const inherit = mount(UserAvatar, { props: { user: { username: 'a' }, tone: 'inherit' } });

    expect(muted.find('[data-slot=avatar-fallback]').classes()).toContain('text-muted-foreground');
    expect(inherit.find('[data-slot=avatar-fallback]').classes()).toContain('text-inherit');
    expect(inherit.find('[data-slot=avatar-fallback]').classes()).not.toContain(
      'text-muted-foreground',
    );
  });
});
