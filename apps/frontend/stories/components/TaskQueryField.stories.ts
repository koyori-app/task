import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, userEvent, within } from 'storybook/test';
import TaskQueryField from '@/components/tasks/query/TaskQueryField.vue';
const meta = {
  title: 'Components/TaskQueryField',
  component: TaskQueryField,
  args: {
    modelValue: '',
    suggestData: {
      labels: [
        { id: 'bug', name: 'bug' },
        { id: 'backend', name: 'backend' },
      ],
    },
  },
  render: (args) => ({
    components: { TaskQueryField },
    setup: () => ({ args }),
    template: '<TaskQueryField v-bind="args" @update:model-value="args.modelValue=$event"/>',
  }),
} satisfies Meta<typeof TaskQueryField>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Empty: Story = {};
export const WithChips: Story = { args: { modelValue: 'status:"In Progress" -label:保留 通知' } };
export const ValueSuggest: Story = {
  ...WithChips,
  play: async ({ canvasElement }) => {
    const input = within(canvasElement).getByRole('combobox');
    await userEvent.clear(input);
    await userEvent.type(input, 'label:b');
  },
};
/** 値の候補を選ぶと、カンマを足して検索は確定せず、残りの候補（backend）を出したままにする。 */
export const MultiValuePick: Story = {
  ...WithChips,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const input = canvas.getByRole('combobox');
    await userEvent.clear(input);
    await userEvent.type(input, 'label:b');
    await userEvent.keyboard('{ArrowDown}{Enter}');
    await expect(input).toHaveValue('label:bug,');
    await expect(await canvas.findByRole('option', { name: /backend/ })).toBeVisible();
  },
};
export const ManyChips: Story = {
  args: {
    modelValue:
      'status:Todo label:bug assignee:@me priority:high due:<7d no:label in:title sort:priority',
  },
};
