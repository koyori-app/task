import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, userEvent, within } from 'storybook/test';
import { ref } from 'vue';
import PasswordInput from '@/components/auth/PasswordInput.vue';

const meta = {
  title: 'Auth/PasswordInput',
  component: PasswordInput,
  tags: ['autodocs'],
  render: (args: Record<string, unknown>) => ({
    components: { PasswordInput },
    setup() {
      const model = ref((args.modelValue as string) ?? '');
      return { args, model };
    },
    template: '<PasswordInput v-bind="args" v-model="model" />',
  }),
  args: {
    id: 'password',
    name: 'password',
    placeholder: 'パスワード',
    autocomplete: 'current-password',
    modelValue: 'secret-password',
  },
} satisfies Meta<typeof PasswordInput>;

export default meta;
type Story = StoryObj<typeof meta>;

export const PasswordHidden: Story = {
  name: 'type=password（デフォルト）',
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const input = canvas.getByDisplayValue('secret-password');
    await expect(input).toHaveAttribute('type', 'password');
  },
};

export const PasswordVisible: Story = {
  name: 'type=text（表示中）',
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const toggle = canvas.getByRole('button', { name: 'パスワードを表示する' });
    await userEvent.click(toggle);
    const input = canvas.getByDisplayValue('secret-password');
    await expect(input).toHaveAttribute('type', 'text');
  },
};
