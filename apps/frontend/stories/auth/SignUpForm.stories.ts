import type { Meta, StoryObj } from '@storybook/vue3-vite';

import SignUpForm from '@/components/auth/SignUpForm.vue';

const meta = {
  title: 'Auth/SignUpForm',
  component: SignUpForm,
  tags: ['autodocs'],
  parameters: {
    layout: 'centered',
  },
} satisfies Meta<typeof SignUpForm>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};
