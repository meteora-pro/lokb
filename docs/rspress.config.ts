import { defineConfig } from "@rspress/core";
import path from "node:path";

export default defineConfig({
  root: path.join(__dirname, "guide"),
  title: "lokb",
  description: "Local Offline Knowledge Base — personal offline knowledge library built with Rust",
  base: process.env.DOCS_BASE_PATH || "/",
  globalStyles: path.join(__dirname, "styles", "index.css"),
  locales: [
    {
      lang: "en",
      label: "English",
    },
    {
      lang: "ru",
      label: "Русский",
    },
  ],
  themeConfig: {
    socialLinks: [
      {
        icon: "github",
        mode: "link",
        content: "https://github.com/meteora-pro/lokb",
      },
    ],
  },
});
