import type { Meta, StoryObj } from '@storybook/vue3-vite';
import SignInForm from '@/components/auth/SignInForm.vue';

const meta = {
  title: 'Auth/SignInForm',
  component: SignInForm,
  tags: ['autodocs'],
  parameters: {
    docs: {
      description: {
        component: 'onSubmit は console.log スタブ（API 未接続）。',
      },
    },
  },
} satisfies Meta<typeof SignInForm>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};
