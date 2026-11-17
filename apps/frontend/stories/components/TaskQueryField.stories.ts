import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { userEvent, within } from 'storybook/test';
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
export const ManyChips: Story = {
  args: {
    modelValue:
      'status:Todo label:bug assignee:@me priority:high due:<7d no:label in:title sort:priority',
  },
};
