import type { Meta, StoryObj } from '@storybook/vue3-vite';
import TaskFilterBar from '@/components/tasks/query/TaskFilterBar.vue';
const meta = {
  title: 'Components/TaskFilterBar',
  component: TaskFilterBar,
  args: {
    modelValue: '',
    statuses: [{ id: '1', name: 'In Progress' }],
    labels: [{ id: '1', name: '保留' }],
    members: [],
  },
  render: (args) => ({
    components: { TaskFilterBar },
    setup: () => ({ args }),
    template: '<TaskFilterBar v-bind="args" @update:model-value="args.modelValue=$event"/>',
  }),
} satisfies Meta<typeof TaskFilterBar>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {};
export const Active: Story = { args: { modelValue: 'status:"In Progress" -label:保留' } };
