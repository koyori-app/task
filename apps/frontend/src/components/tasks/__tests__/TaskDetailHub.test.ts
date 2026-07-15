import { afterEach, describe, expect, it } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';
import type { components } from '@/generated/api';
import TaskDetailHub from '../TaskDetailHub.vue';

enableAutoUnmount(afterEach);

type TaskDetail = components['schemas']['TaskDetailResponse'];

const task: TaskDetail = {
  assignees: [],
  created_at: '2026-07-16T00:00:00Z',
  custom_field_values: [],
  id: 'task-id',
  is_archived: false,
  priority: 'Medium',
  progress_pct: 30,
  project_id: 'project-id',
  seq_id: 1,
  status_id: 'status-id',
  title: 'Test task',
  updated_at: '2026-07-16T00:00:00Z',
};

describe('TaskDetailHub', () => {
  it('does not save an empty progress value as zero percent', async () => {
    const wrapper = mount(TaskDetailHub, {
      props: {
        task,
        projectKey: 'TEST',
        statuses: [],
        statusId: task.status_id,
      },
    });

    const progressButton = wrapper.findAll('button').find((button) => button.text() === '30%');
    expect(progressButton).toBeDefined();
    await progressButton!.trigger('click');
    const progressInput = wrapper.get('input[aria-label="進捗率"]');
    await progressInput.setValue('');
    await progressInput.trigger('blur');

    expect(wrapper.emitted('save:progress_pct')).toBeUndefined();
    expect(wrapper.find('input[aria-label="進捗率"]').exists()).toBe(false);
    expect(wrapper.text()).toContain('30%');
  });
});
