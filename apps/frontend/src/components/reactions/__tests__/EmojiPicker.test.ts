import { flushPromises, mount } from '@vue/test-utils';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { loadEmojiData, RECENT_EMOJI_STORAGE_KEY } from '@/lib/emoji-data';

import EmojiPicker from '../EmojiPicker.vue';
import ReactionAddButton from '../ReactionAddButton.vue';

async function mountPicker(props: { selected?: string[] } = {}) {
  const wrapper = mount(EmojiPicker, { attachTo: document.body, props });
  await flushPromises();
  return wrapper;
}

type Picker = Awaited<ReturnType<typeof mountPicker>>;

async function search(wrapper: Picker, query: string) {
  await wrapper.get('input').setValue(query);
  await flushPromises();
}

function cellLabels(wrapper: Picker) {
  return wrapper.findAll('[role="gridcell"]').map((cell) => cell.attributes('aria-label'));
}

function cellEmoji(wrapper: Picker) {
  return wrapper.findAll('[role="gridcell"]').map((cell) => cell.text());
}

describe('EmojiPicker', () => {
  beforeAll(async () => {
    await loadEmojiData();
  });

  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    document.body.innerHTML = '';
  });

  it('先頭に「よく使う」16 個を置き、その下にカテゴリを続ける', async () => {
    const wrapper = await mountPicker();
    const sections = wrapper.findAll('[role="rowgroup"]');
    expect(sections[0]?.get('[role="columnheader"]').text()).toBe('よく使う');
    expect(sections[0]?.findAll('[role="gridcell"]')).toHaveLength(16);
    expect(sections[1]?.get('[role="columnheader"]').text()).toBe('スマイリーと感情');
    expect(cellEmoji(wrapper).slice(0, 2)).toEqual(['👍', '❤️']);
  });

  it('Unicode 15 より新しい絵文字と肌の色の違いは一覧に出さない', async () => {
    const wrapper = await mountPicker();
    const emoji = cellEmoji(wrapper);
    expect(emoji).not.toContain('🫩'); // Unicode 16
    expect(emoji).not.toContain('🙂‍↔️'); // Unicode 15.1
    expect(emoji).toContain('🫨'); // Unicode 15
    expect(emoji).not.toContain('👍🏽');
    expect(emoji).not.toContain('🏽');
  });

  it('localStorage が使えなくても既定の並びで動き、選べる', async () => {
    vi.stubGlobal('localStorage', {
      getItem: () => {
        throw new Error('blocked');
      },
      setItem: () => {
        throw new Error('blocked');
      },
    });
    const wrapper = await mountPicker();
    expect(cellEmoji(wrapper)[0]).toBe('👍');
    await wrapper.get('[role="gridcell"]').trigger('click');
    expect(wrapper.emitted('select')).toEqual([['👍']]);
  });

  it('壊れた localStorage の中身でも既定の並びで動く', async () => {
    localStorage.setItem(RECENT_EMOJI_STORAGE_KEY, 'not json');
    const wrapper = await mountPicker();
    expect(cellEmoji(wrapper)[0]).toBe('👍');
  });

  it('使うほど頻度順に変わるが、開いている間は並びを変えない', async () => {
    const wrapper = await mountPicker();
    const fire = wrapper.findAll('[role="gridcell"]').find((cell) => cell.text() === '🔥');
    await fire?.trigger('click');
    expect(wrapper.emitted('select')).toEqual([['🔥']]);
    expect(cellEmoji(wrapper)[0]).toBe('👍');
    wrapper.unmount();

    const reopened = await mountPicker();
    expect(cellEmoji(reopened)[0]).toBe('🔥');
  });

  it('日本語名・タグ・shortcode の部分一致で検索し、大文字小文字を区別しない', async () => {
    const wrapper = await mountPicker();
    await search(wrapper, 'サムズ');
    expect(cellLabels(wrapper)).toContain('サムズアップ');
    await search(wrapper, 'いいね'); // タグ
    expect(cellLabels(wrapper)).toContain('サムズアップ');
    await search(wrapper, 'THUMBS'); // shortcode
    expect(cellLabels(wrapper)).toContain('サムズアップ');
  });

  it('検索中は左の列を隠し、「検索結果 N 件」の 1 段にする', async () => {
    const wrapper = await mountPicker();
    expect(wrapper.find('[aria-label="カテゴリ"]').exists()).toBe(true);
    await search(wrapper, 'サムズアップ');
    expect(wrapper.find('[aria-label="カテゴリ"]').exists()).toBe(false);
    expect(wrapper.findAll('[role="rowgroup"]')).toHaveLength(1);
    const count = wrapper.findAll('[role="gridcell"]').length;
    expect(wrapper.get('[role="columnheader"]').text()).toBe(`検索結果 ${count} 件`);
  });

  it('結果が 0 件なら案内を出す', async () => {
    const wrapper = await mountPicker();
    await search(wrapper, 'そんなえもじはない');
    expect(wrapper.text()).toContain('絵文字が見つかりません');
    expect(wrapper.text()).toContain('別の言葉で検索してください');
    expect(wrapper.find('[role="grid"]').exists()).toBe(false);
  });

  it('検索欄の Enter は先頭の結果を選ぶ。変換確定の Enter では選ばない', async () => {
    const wrapper = await mountPicker();
    await search(wrapper, 'サムズアップ');
    await wrapper.get('input').trigger('keydown', { key: 'Enter', isComposing: true });
    expect(wrapper.emitted('select')).toBeUndefined();
    await wrapper.get('input').trigger('keydown', { key: 'Enter' });
    expect(wrapper.emitted('select')).toEqual([['👍']]);
  });

  it('矢印で移動し、Enter で選ぶ。読み上げは日本語名で、名前欄も追従する', async () => {
    const wrapper = await mountPicker();
    await wrapper.get('input').trigger('keydown', { key: 'ArrowDown' });
    await flushPromises();
    expect(document.activeElement?.getAttribute('aria-label')).toBe('サムズアップ');

    const grid = wrapper.get('[role="grid"]');
    await grid.trigger('keydown', { key: 'ArrowRight' });
    await flushPromises();
    expect(document.activeElement?.getAttribute('aria-label')).toBe('赤いハート');
    expect(wrapper.get('[data-testid="emoji-preview"]').text()).toContain('赤いハート');

    await grid.trigger('keydown', { key: 'ArrowDown' });
    await flushPromises();
    await grid.trigger('keydown', { key: 'ArrowUp' });
    await flushPromises();
    expect(document.activeElement?.getAttribute('aria-label')).toBe('赤いハート');

    await grid.trigger('keydown', { key: 'Enter' });
    expect(wrapper.emitted('select')).toEqual([['❤️']]);
  });

  it('名前欄に日本語名と :shortcode: を出す', async () => {
    const wrapper = await mountPicker();
    await wrapper.get('[aria-label="サムズアップ"]').trigger('pointerover');
    expect(wrapper.get('[data-testid="emoji-preview"]').text()).toContain('サムズアップ');
    expect(wrapper.get('[data-testid="emoji-preview"] code').text()).toMatch(/^:.+:$/);
  });

  it('自分が押している絵文字の升を示す（異体字セレクタの有無を問わない）', async () => {
    const wrapper = await mountPicker({ selected: ['❤'] });
    const [cell] = wrapper.findAll('[aria-label="赤いハート"]');
    expect(cell?.attributes('aria-selected')).toBe('true');
    expect(wrapper.findAll('[aria-label="サムズアップ"]')[0]?.attributes('aria-selected')).toBe(
      'false',
    );
  });

  it('Esc は close を出し、外の dialog へは伝えない', async () => {
    const wrapper = await mountPicker();
    const event = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });
    wrapper.get('input').element.dispatchEvent(event);
    expect(wrapper.emitted('close')).toHaveLength(1);
    expect(event.defaultPrevented).toBe(true);
  });
});

describe('ReactionAddButton', () => {
  beforeAll(async () => {
    await loadEmojiData();
  });

  afterEach(() => {
    document.body.innerHTML = '';
  });

  function mountButton() {
    return mount(ReactionAddButton, { attachTo: document.body });
  }

  it('Esc で閉じて開いたボタンへフォーカスを戻す', async () => {
    const wrapper = mountButton();
    await wrapper.get('button').trigger('click');
    await flushPromises();
    expect(wrapper.findComponent(EmojiPicker).exists()).toBe(true);

    await wrapper.get('input').trigger('keydown', { key: 'Escape' });
    expect(wrapper.findComponent(EmojiPicker).exists()).toBe(false);
    expect(document.activeElement).toBe(wrapper.get('[aria-haspopup]').element);
  });

  it('選ぶと select を出して閉じ、ボタンへ戻す', async () => {
    const wrapper = mountButton();
    await wrapper.get('button').trigger('click');
    await flushPromises();
    await wrapper.get('[aria-label="サムズアップ"]').trigger('click');
    expect(wrapper.emitted('select')).toEqual([['👍']]);
    expect(wrapper.findComponent(EmojiPicker).exists()).toBe(false);
    expect(document.activeElement).toBe(wrapper.get('[aria-haspopup]').element);
  });

  it('dialog の背景を押した 1 回目はピッカーだけを閉じ、dialog へ届けない', async () => {
    const dialog = document.createElement('dialog');
    document.body.append(dialog);
    const reachedDialog = vi.fn();
    dialog.addEventListener('pointerdown', reachedDialog);
    const wrapper = mountButton();
    await wrapper.get('button').trigger('click');
    await flushPromises();

    dialog.dispatchEvent(new Event('pointerdown', { bubbles: true }));
    await flushPromises();
    expect(wrapper.findComponent(EmojiPicker).exists()).toBe(false);
    expect(reachedDialog).not.toHaveBeenCalled();

    // 閉じた後は普通に届く
    dialog.dispatchEvent(new Event('pointerdown', { bubbles: true }));
    expect(reachedDialog).toHaveBeenCalledTimes(1);
  });

  it('外側を押すと閉じる（フォーカスは奪わない）', async () => {
    const wrapper = mountButton();
    await wrapper.get('button').trigger('click');
    await flushPromises();
    document.body.dispatchEvent(new Event('pointerdown', { bubbles: true }));
    await flushPromises();
    expect(wrapper.findComponent(EmojiPicker).exists()).toBe(false);
  });
});
