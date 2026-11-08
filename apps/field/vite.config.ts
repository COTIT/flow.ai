import { defineConfig } from 'vite'
import { fileURLToPath } from 'node:url'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { tanstackRouter } from '@tanstack/router-plugin/vite'
import { VitePWA } from 'vite-plugin-pwa'

// Same variable as the Next apps (e.g. http://localhost:8080/v1); the proxy keeps the /v1 path.
const apiOrigin = new URL(process.env.API_URL ?? 'http://localhost:8080/v1').origin

export default defineConfig({
  plugins: [
    // must precede the react plugin: it generates routeTree.gen.ts
    tanstackRouter({ target: 'react', autoCodeSplitting: true }),
    react(),
    tailwindcss(),
    VitePWA({
      registerType: 'autoUpdate',
      // App shell only. No runtime caching of API responses: the API decides
      // what a user may see, and a cached response outlives a revoked
      // membership. Offline data is deliberately out of scope (Plan v2).
      workbox: {
        globPatterns: ['**/*.{js,css,html,woff2,svg,png,ico}'],
        navigateFallback: '/index.html',
      },
      manifest: {
        name: 'Ruaha 360',
        short_name: 'Ruaha 360',
        description: 'Rural economic development platform for Ruaha Energy',
        lang: 'sw',
        start_url: '/',
        display: 'standalone',
        background_color: '#F5F3E5',
        theme_color: '#1d70b7',
        // Rendered by packages/e2e/scripts/make-app-icons.mjs; Android needs 192 + 512 to offer an install.
        icons: [
          { src: '/icons/icon-192.png', sizes: '192x192', type: 'image/png' },
          { src: '/icons/icon-512.png', sizes: '512x512', type: 'image/png' },
          { src: '/icons/icon-maskable-512.png', sizes: '512x512', type: 'image/png', purpose: 'maskable' },
        ],
      },
    }),
  ],
  // The browser calls same-origin /v1 (the API sends no CORS headers); Traefik does this in prod.
  server: { proxy: { '/v1': apiOrigin } },
  preview: { proxy: { '/v1': apiOrigin } },
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
  },
})
