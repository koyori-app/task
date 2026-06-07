import createClient from 'openapi-fetch';
import type { paths } from '@/generated/api';

export const apiClient = createClient<paths>({
  baseUrl: import.meta.env.VITE_API_BASE ?? '/api',
});
