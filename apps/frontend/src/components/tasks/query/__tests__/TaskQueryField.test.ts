import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import TaskQueryField from '../TaskQueryField.vue';
describe('Enter confirms the task query', () => {
  it('does not commit while typing or composing, commits on Enter', async () => {
    const w = mount(TaskQueryField, { props: { modelValue: '' } });
    const input = w.get('input');
    await input.setValue('通知');
    expect(w.emitted('update:modelValue')).toBeUndefined();
    await input.trigger('keydown', { key: 'Enter', isComposing: true });
    expect(w.emitted('update:modelValue')).toBeUndefined();
    await input.trigger('keydown', { key: 'Enter', keyCode: 229 });
    expect(w.emitted('update:modelValue')).toBeUndefined();
    await input.trigger('keydown', { key: 'Enter' });
    expect(w.emitted('update:modelValue')?.[0]).toEqual(['通知']);
    w.unmount();
  });
  it('keeps key prefixes as words on Enter, while values select only an arrowed candidate', async () => {
    const w = mount(TaskQueryField, {
      props: { modelValue: '', suggestData: { labels: [{ id: '1', name: 'bug' }] } },
    });
    const input = w.get('input');
    await input.trigger('focusin');
    await input.setValue('la');
    await input.trigger('keydown', { key: 'Enter' });
    expect(w.emitted('update:modelValue')?.at(-1)).toEqual(['la']);
    await input.setValue('label:b');
    await input.trigger('keydown', { key: 'ArrowDown' });
    await input.trigger('keydown', { key: 'Enter' });
    expect(w.emitted('update:modelValue')?.at(-1)).toEqual(['label:bug']);
    w.unmount();
  });
  const suggestData = {
    labels: [
      { id: '1', name: 'bug' },
      { id: '2', name: 'bugfix' },
    ],
    members: [{ id: '3', username: 'alice' }],
  };
  it.each([
    'priority:critical',
    'priority:hi',
    'label:wontfix',
    'label:bu',
    'label:x',
    'assignee:@ali',
    'assignee:@bob',
  ])('keeps %s as typed on Enter (no silent replacement)', async (value) => {
    const w = mount(TaskQueryField, { props: { modelValue: '', suggestData } });
    await w.get('input').trigger('focusin');
    await w.get('input').setValue(value);
    await w.get('input').trigger('keydown', { key: 'Enter' });
    expect(w.emitted('update:modelValue')?.at(-1)).toEqual([value]);
    w.unmount();
  });
  it.each([
    ['assignee:@alice', 'assignee:alice'],
    ['author:@ALICE', 'author:alice'],
    ['label:BUG', 'label:bug'],
  ])('confirms the exact match of %s as %s', async (value, expected) => {
    const w = mount(TaskQueryField, { props: { modelValue: '', suggestData } });
    await w.get('input').trigger('focusin');
    await w.get('input').setValue(value);
    await w.get('input').trigger('keydown', { key: 'Enter' });
    expect(w.emitted('update:modelValue')?.at(-1)).toEqual([expected]);
    w.unmount();
  });
  it('opens with the exact match selected and lists it first', async () => {
    const w = mount(TaskQueryField, { props: { modelValue: '', suggestData } });
    await w.get('input').trigger('focusin');
    await w.get('input').setValue('label:BUGFIX');
    const options = w.findAll('[role="option"]');
    expect(options[0]!.text()).toContain('bugfix');
    expect(options[0]!.attributes('aria-selected')).toBe('true');
    // 「ラベルなし」の行は一致が無くても並ぶが、自動では選ばない
    await w.get('input').setValue('label:wontfix');
    const rows = w.findAll('[role="option"]');
    expect(rows).toHaveLength(1);
    expect(rows[0]!.attributes('aria-selected')).not.toBe('true');
    w.unmount();
  });
  it('replaces the input with the arrowed candidate only', async () => {
    const w = mount(TaskQueryField, { props: { modelValue: '', suggestData } });
    await w.get('input').trigger('focusin');
    await w.get('input').setValue('label:bu');
    await w.get('input').trigger('keydown', { key: 'ArrowDown' });
    await w.get('input').trigger('keydown', { key: 'ArrowDown' });
    await w.get('input').trigger('keydown', { key: 'Enter' });
    expect(w.emitted('update:modelValue')?.at(-1)).toEqual(['label:bugfix']);
    w.unmount();
  });
  it('ignores Enter on inner buttons', async () => {
    const w = mount(TaskQueryField, { props: { modelValue: 'label:bug' } });
    const event = new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true });
    w.get('button[aria-label="書き方の早見表"]').element.dispatchEvent(event);
    w.get('button[aria-label="label:bug を外す"]').element.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }),
    );
    expect(event.defaultPrevented).toBe(false);
    expect(w.emitted('update:modelValue')).toBeUndefined();
    w.unmount();
  });
  it('closes candidates on Escape and prevents the search field from clearing', async () => {
    const w = mount(TaskQueryField, { props: { modelValue: '', suggestData } });
    await w.get('input').trigger('focusin');
    await w.get('input').setValue('label:x');
    expect(w.find('[role="listbox"]').exists()).toBe(true);
    const event = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });
    w.get('input').element.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
    await w.vm.$nextTick();
    expect(w.find('[role="listbox"]').exists()).toBe(false);
    expect(w.get('input').element.value).toBe('label:x');
    w.unmount();
  });
  it('removes chips by button and leading Backspace', async () => {
    const w = mount(TaskQueryField, { props: { modelValue: 'status:Todo label:bug' } });
    await w.get('button[aria-label="label:bug を外す"]').trigger('click');
    expect(w.emitted('update:modelValue')?.at(-1)).toEqual(['status:Todo']);
    await w.setProps({ modelValue: 'status:Todo' });
    await w.setProps({ modelValue: 'status:Todo label:bug' });
    w.get('input').element.setSelectionRange(0, 0);
    await w.get('input').trigger('keydown', { key: 'Backspace' });
    expect(w.emitted('update:modelValue')?.at(-1)).toEqual(['status:Todo']);
    w.unmount();
  });
  it('restores external expressions and shows syntax help', async () => {
    const w = mount(TaskQueryField, { props: { modelValue: 'status:Todo 通知' } });
    expect(w.text()).toContain('status:Todo');
    expect(w.get('input').element.value).toBe('通知');
    await w.setProps({ modelValue: 'label:bug' });
    expect(w.text()).toContain('label:bug');
    await w.get('button[aria-label="書き方の早見表"]').trigger('click');
    expect(w.text()).toContain('括弧と OR は使えません');
    await w.get('input').trigger('keydown', { key: 'Escape' });
    expect(w.text()).not.toContain('括弧と OR は使えません');
    w.unmount();
  });
});
