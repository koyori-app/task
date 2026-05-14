import type { Config } from "vike/types";
import vikeVue from "vike-vue/config";

// Default config (can be overridden by pages)
// https://vike.dev/config

const config: Config = {
  // https://vike.dev/head-tags
  extends: [vikeVue],
  title: "My Vike App",
  description: "Demo showcasing Vike",
  lang: "ja",

};

export default config;
