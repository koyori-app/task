import { useRuntimeConfig } from '#app';
import { Configuration, DefaultApi } from '@/utils/openapi/index';

/**
 * OpenAPI 生成クライアント。basePath は runtimeConfig.public.apiBase（既定 /api、NUXT_PUBLIC_API_BASE で上書き可）。
 */
export function useDefaultApi(): DefaultApi {
  const runtimeConfig = useRuntimeConfig();
  return new DefaultApi(
    new Configuration({
      basePath: runtimeConfig.public.apiBase,
      credentials: 'include',
    }),
  );
}
