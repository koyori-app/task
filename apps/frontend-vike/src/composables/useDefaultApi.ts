import { createApi } from '@/lib/api';
import type { DefaultApi } from '@/generated/api';

export function useDefaultApi(): DefaultApi {
  return createApi();
}
