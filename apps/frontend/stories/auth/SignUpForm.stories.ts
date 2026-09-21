import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { provide } from 'vue';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';

import SignUpForm from '@/components/auth/SignUpForm.vue';

const meta = {
  title: 'Auth/SignUpForm',
  component: SignUpForm,
  tags: ['autodocs'],
  parameters: {
    layout: 'centered',
  },
  decorators: [
    // Shown bare, this centered story has no width context; production SignUp.vue wraps the
    // card in `w-full max-w-sm md:max-w-4xl`. Without it, Storybook's embed-sizing autosize
    // (`#storybook-root > * { width: fit-content }`) collapses the card into a
    // one-character-per-line column. Re-supply the md width (56rem = max-w-4xl) so the story
    // renders as the production card.
    () => ({
      setup() {
        const queryClient = new QueryClient({
          defaultOptions: {
            queries: { retry: false, gcTime: 0, staleTime: 0 },
            mutations: { retry: false },
          },
        });
        provide(VUE_QUERY_CLIENT, queryClient);
      },
      template: '<div style="width: 56rem; max-width: 100%; margin-inline: auto;"><story /></div>',
    }),
  ],
} satisfies Meta<typeof SignUpForm>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};
