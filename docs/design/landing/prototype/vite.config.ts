import react from "@vitejs/plugin-react"
import tailwindcss from "@tailwindcss/vite"
import { defineConfig } from "vite"

// Fonts and logos come from the repo (packages/ui fonts, brand SVGs in ../assets).
export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: { fs: { allow: ["../../../.."] } },
  resolve: { alias: { "@": "/src" } },
})
