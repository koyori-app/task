import type { Meta, StoryObj } from '@storybook/vue3-vite';
import PasswordStrengthBar from '@/components/auth/PasswordStrengthBar.vue';

const meta = {
  title: 'Auth/PasswordStrengthBar',
  component: PasswordStrengthBar,
  tags: ['autodocs'],
  args: {
    strength: '',
  },
} satisfies Meta<typeof PasswordStrengthBar>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Empty: Story = {
  name: "strength ''",
  args: { strength: '' },
};

export const Low: Story = {
  name: "strength 'low'",
  args: { strength: 'low' },
};

export const Medium: Story = {
  name: "strength 'medium'",
  args: { strength: 'medium' },
};

export const High: Story = {
  name: "strength 'high'",
  args: { strength: 'high' },
};
