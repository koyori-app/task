import arkenv from "arkenv";

export const config = arkenv({
  // Main System
  APP_URL: "string.url",

  // Umami Analytics
  "UMAMI_HOST?": "string.url",
  "UMAMI_WEBSITE_ID?": "string.uuid.v4",

  // @nuxtjs/og-image
  "NUXT_OG_IMAGE_SECRET?": "string",
});
