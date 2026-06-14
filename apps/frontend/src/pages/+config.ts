import type { Config } from "vike/types";
import vikeVue from "vike-vue/config";
import vikeVuePinia from "vike-vue-pinia/config";
import vikeVueQuery from "vike-vue-query/config";

// Default config (can be overridden by pages)
// https://vike.dev/config

const config: Config = {
  // https://vike.dev/head-tags
  extends: [vikeVue, vikeVuePinia, vikeVueQuery],
  title: "My Vike App",
  description: "Demo showcasing Vike",
  lang: "ja",
};

export default config;
