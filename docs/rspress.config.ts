import { defineConfig } from "@rspress/core";
import path from "node:path";

export default defineConfig({
  root: path.join(__dirname, "guide"),
  title: "lokb",
  description:
    "Local Offline Knowledge Base — персональная offline библиотека знаний на Rust",
  base: process.env.DOCS_BASE_PATH || "/",
  globalStyles: path.join(__dirname, "styles", "index.css"),
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
