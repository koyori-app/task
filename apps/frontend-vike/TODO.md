Follow the steps below to finish setting up your application.

## Sentry Browser / Error Tracking & Performance Monitoring

Add your Sentry DSN to `.env` file.
You can configure [Sentry for the browser](https://docs.sentry.io/platforms/javascript/guides/vue/) in `sentry.browser.config.ts`.

Upload of source maps to Sentry is handled by the [`sentryVitePlugin`](https://docs.sentry.io/platforms/javascript/sourcemaps/uploading/vite/) in `vite.config.ts`.
You have to configure `SENTRY_ORG`, `SENTRY_PROJECT` and `SENTRY_AUTH_TOKEN` in the `.env.sentry-build-plugin` file with the values from your Sentry account.

