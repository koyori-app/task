import { afterEach, describe, expect, it } from 'vitest';
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils';
import { defineComponent } from 'vue';
import { Button } from '@koyori-app/ui-vue';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';

/*
 * Reka の trigger（as-child）が koyori の Button に onClick / onKeydown / aria-* を流し込み、
 * メニューが開くことを確かめる。Button は onClick を prop で受けるので、ここが壊れると
 * クリックもキーボードも効かなくなる。
 */
const Menu = defineComponent({
  components: { Button, DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger },
  template: `
    <DropdownMenu>
      <DropdownMenuTrigger as-child>
        <Button variant="ghost" ariaLabel="タスク操作" />
      </DropdownMenuTrigger>
      <DropdownMenuContent>
        <DropdownMenuItem>削除</DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  `,
});

let wrapper: VueWrapper | undefined;
afterEach(() => wrapper?.unmount());

async function mountMenu() {
  wrapper = mount(Menu, { attachTo: document.body });
  await flushPromises();
  const button = wrapper.get('button[aria-label="タスク操作"]');
  expect(button.attributes('aria-haspopup')).toBe('menu');
  expect(document.body.querySelector('[role="menu"]')).toBeNull();
  return button;
}

describe('koyori の Button を Reka の DropdownMenuTrigger に入れる', () => {
  it('クリックでメニューが開き、開いた状態を aria-expanded で伝える', async () => {
    const button = await mountMenu();

    await button.trigger('click');
    await flushPromises();

    expect(document.body.querySelector('[role="menu"]')?.textContent).toContain('削除');
    expect(button.attributes('aria-expanded')).toBe('true');
    expect(button.attributes('data-state')).toBe('open');
  });

  it.each(['Enter', ' ', 'ArrowDown'])('%j キーでメニューが開く', async (key) => {
    const button = await mountMenu();

    await button.trigger('keydown', { key });
    await flushPromises();

    expect(document.body.querySelector('[role="menu"]')).not.toBeNull();
  });
});
