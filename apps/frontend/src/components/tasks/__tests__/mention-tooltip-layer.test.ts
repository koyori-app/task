// @vitest-environment happy-dom
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';

import CommentEditor from '../CommentEditor.vue';

/**
 * メンションの候補の箱（`.cm-tooltip-autocomplete`）の置き場と段を、実物の編集器で縛る（TASK-412）。
 *
 * Safari では CodeMirror が候補の箱を `position: absolute` に切り替え、箱を編集器の中に置く。
 * 編集器の枠は `overflow: hidden` なので、箱が枠で切られていた。
 * 箱を枠の外（dialog の外なら body、中ならその dialog）へ出し、ui-vue の menu の段に載せる。
 *
 * 説明の欄（MarkdownEditor）も同じ mentionExtensions を使うが、view を Vue の ref に持つため、
 * happy-dom では ref の proxy 越しに CodeMirror を動かせない（comment-editor-stub.ts と同じ事情）。
 * 説明の欄の置き場は実の browser で確かめる。
 */
const CANDIDATES = [
  { id: '11111111-1111-4111-8111-111111111111', username: 'alice' },
  { id: '22222222-2222-4222-8222-222222222222', username: 'albert' },
];

enableAutoUnmount(afterEach);

beforeAll(() => {
  // ui-vue の CSS が置く menu の段。試験の環境には CSS が無いので同じ値を置く
  document.documentElement.style.setProperty('--koyori-z-menu', '10');
});

type Picker = { openMentionPicker: () => void };

async function openPicker(vm: Picker) {
  vm.openMentionPicker();
  return vi.waitFor(() => {
    const tooltip = document.querySelector<HTMLElement>('.cm-tooltip-autocomplete');
    expect(tooltip).not.toBeNull();
    return tooltip!;
  });
}

/** CodeMirror は箱を、親に置いた器（position: relative の div）の中へ入れる */
function holderOf(tooltip: HTMLElement) {
  return tooltip.parentElement!.parentElement;
}

describe('メンションの候補の箱の置き場と段', () => {
  it('comment の欄では、箱を編集器の外の body の直下の器へ出し、menu の段に載せる', async () => {
    const wrapper = mount(CommentEditor, {
      props: { modelValue: '', candidates: CANDIDATES },
      attachTo: document.body,
    });
    const tooltip = await openPicker(wrapper.vm as unknown as Picker);

    expect(wrapper.element.contains(tooltip)).toBe(false);
    expect(holderOf(tooltip)).toBe(document.body);
    expect(getComputedStyle(tooltip).zIndex).toBe('10');
  });

  it('dialog の中の欄では、箱をその dialog の直下の器へ出す（body だと top layer の dialog の下に隠れる）', async () => {
    const dialog = document.createElement('dialog');
    document.body.append(dialog);
    const wrapper = mount(CommentEditor, {
      props: { modelValue: '', candidates: CANDIDATES },
      attachTo: dialog,
    });
    const tooltip = await openPicker(wrapper.vm as unknown as Picker);

    expect(wrapper.element.contains(tooltip)).toBe(false);
    expect(holderOf(tooltip)).toBe(dialog);
    wrapper.unmount();
    dialog.remove();
  });

  it('欄を外すと、body へ出した器も一緒に消え、箱が取り残されない', async () => {
    const before = document.body.childElementCount;
    const wrapper = mount(CommentEditor, {
      props: { modelValue: '', candidates: CANDIDATES },
      attachTo: document.body,
    });
    await openPicker(wrapper.vm as unknown as Picker);

    wrapper.unmount();

    expect(document.querySelector('.cm-tooltip-autocomplete')).toBeNull();
    expect(document.body.childElementCount).toBe(before);
  });
});
