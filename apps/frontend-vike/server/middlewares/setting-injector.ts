import {
  enhance,
  MiddlewareOrder,
  type UniversalMiddleware,
} from "@universal-middleware/core";
import { appEnvSettings } from "../settings/env";

type AppSettings = {
  env: typeof appEnvSettings;
};

type SettingsContext = Universal.Context & {
  settings: AppSettings;
};

export const appSettings = {
  env: appEnvSettings,
} satisfies AppSettings;

const injectSettings: UniversalMiddleware<Universal.Context, SettingsContext> = (
  _request,
  context,
) => ({
  ...context,
  settings: appSettings,
});

export const settingInjector = enhance(injectSettings, {
  name: "app:setting-injector",
  order: MiddlewareOrder.CUSTOM_PRE_PROCESSING,
  immutable: true,
});

declare global {
  namespace Universal {
    interface Context {
      settings?: AppSettings;
    }
  }

  namespace Vike {
    interface PageContext {
      settings: AppSettings;
    }
  }
}
