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
  it('keeps key prefixes as words on Enter, while values select the first candidate', async () => {
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
  it.each(['priority:critical', 'label:wontfix', 'assignee:@alice', 'label:x'])(
    'preserves %s on Enter',
    async (value) => {
      const w = mount(TaskQueryField, { props: { modelValue: '' } });
      await w.get('input').setValue(value);
      await w.get('input').trigger('keydown', { key: 'Enter' });
      expect(w.emitted('update:modelValue')?.at(-1)).toEqual([value]);
      w.unmount();
    },
  );
  it('ignores button Enter and prevents Escape defaults', async () => {
    const w = mount(TaskQueryField, { props: { modelValue: '' } });
    await w.get('button').trigger('keydown', { key: 'Enter' });
    expect(w.emitted('update:modelValue')).toBeUndefined();
    await w.get('input').setValue('label:x');
    const event = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });
    w.get('input').element.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
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
