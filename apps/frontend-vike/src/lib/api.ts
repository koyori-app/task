import { Configuration, DefaultApi } from '@/generated/api';

export function createApi(): DefaultApi {
  return new DefaultApi(
    new Configuration({
      basePath: import.meta.env.VITE_API_BASE ?? '/api',
    }),
  );
}
