import arkenv from 'arkenv';

export const buildEnv = arkenv({
  'ANALYZE?': 'boolean',
  'VITE_INSPECT?': 'boolean',
  'VITE_DEVTOOLS?': 'boolean',
  'VUE_DEVTOOLS?': 'boolean',
  'FORCE_ENABLE_IN_DEV?': 'boolean',
  'CODER_AGENT_URL?': 'string.url',
  'API_BASE?': 'string',
});
