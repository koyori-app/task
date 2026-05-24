import arkenv from "arkenv";

export const appEnvSettings = arkenv({
  // Main System
  APP_URL: "string.url",

  // Umami Analytics
  "UMAMI_HOST?": "string.url",
  "UMAMI_WEBSITE_ID?": "string.uuid.v4",

  // Sentry
  "FORCE_ENABLE_IN_DEV?": "boolean",
});
