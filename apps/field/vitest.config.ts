import { defineConfig, mergeConfig } from 'vitest/config'
import viteConfig from './vite.config'

export default mergeConfig(
  viteConfig,
  defineConfig({
    test: {
      // RUAHA ran alone; in the monorepo turbo runs several suites at once, and some jsdom form tests passed 5s under load.
      testTimeout: 20_000,
      environment: 'jsdom',
      globals: true,
      setupFiles: ['./src/test-setup.ts'],
      include: ['src/**/*.test.{ts,tsx}'],
      // Unit tests never talk to Supabase. These placeholders let modules
      // that import the client load in CI (which has no .env), and replace
      // any local .env values so a stray call fails fast instead of reaching
      // the demo database.
      env: {
        VITE_SUPABASE_URL: 'http://127.0.0.1:9',
        VITE_SUPABASE_ANON_KEY: 'unit-test-placeholder',
        // The demo banner tests assert demo mode, so it is pinned, not inherited.
        VITE_DATA_MODE: 'demo',
        // The demo build shows the resource chain; off-mode tests mock @/lib/features.
        VITE_FEATURE_RESOURCES: 'on',
      },
      coverage: {
        provider: 'v8',
        reporter: ['text', 'html', 'json', 'json-summary'],
        include: [
          'src/components/ConfirmDialog.tsx',
          'src/features/officer/OfficerEditDialog.tsx',
          'src/features/officer/OfficerRecordScreens.tsx',
          'src/features/officer/PersonDetailScreen.tsx',
          'src/features/officer/RecordEditAction.tsx',
          'src/features/officer/VerifyButton.tsx',
          'src/features/officer/VerifyQueueScreen.tsx',
          'src/features/officer/officerEdit.ts',
          'src/features/officer/personDetail.ts',
          'src/features/officer/recordFocus.ts',
          'src/features/officer/useOfficerEdit.ts',
          'src/features/officer/useOfficerRecords.ts',
          'src/features/officer/usePersonDetail.ts',
          'src/features/officer/useVerifyQueue.ts',
          'src/features/officer/verifyNavigation.ts',
        ],
        thresholds: { lines: 100, functions: 100, branches: 100, statements: 100 },
      },
    },
  }),
)
