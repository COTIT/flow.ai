import { defineConfig } from 'vite'
import { fileURLToPath } from 'node:url'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { tanstackRouter } from '@tanstack/router-plugin/vite'
import { VitePWA } from 'vite-plugin-pwa'

export default defineConfig({
  plugins: [
    // must precede the react plugin: it generates routeTree.gen.ts
    tanstackRouter({ target: 'react', autoCodeSplitting: true }),
    react(),
    tailwindcss(),
    VitePWA({
      registerType: 'autoUpdate',
      // App shell only. No runtime caching of Supabase responses: RLS decides
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
      },
    }),
  ],
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
  },
})
