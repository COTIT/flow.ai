import { defineConfig } from '@playwright/test'

import { TOURS_ALREADY_SEEN } from './e2e/tours-already-seen'

// The browser suite, run against the demo Supabase project and its seed.
// `journey.spec.ts` is CLAUDE.md's acceptance journey end to end; every other
// spec covers one screen and its state cycle.
export default defineConfig({
  testDir: './e2e',
  // Serialised on purpose. The suite runs against a single shared Supabase
  // project: five workers signing in concurrently produced intermittent failed
  // reads, which surfaced as a valid officer being routed to /no-access. One
  // worker also makes the language test's write to app_user.locale safe, since
  // nothing else can be mid-read on that row.
  fullyParallel: false,
  workers: 1,
  forbidOnly: !!process.env.CI,
  // One retry absorbs a genuinely transient remote hiccup without hiding a
  // real regression: a test that fails twice in a row still fails.
  retries: 1,
  reporter: 'list',
  // The suite runs against a REMOTE database, and signing in is three
  // sequential round trips to it: the password, then the app_user and
  // membership reads behind `toHaveURL`. Measured at 750–1,615ms on a good
  // connection, so 5s is a thin margin and 10s is not a licence to be slow.
  // This only changes how long a failing assertion waits, not what passes.
  expect: { timeout: 10_000 },
  // The suite creates real records. Both hooks remove anything carrying the
  // E2E- marker, so the seeded figures rls_test.sql asserts stay intact even
  // if a run crashes mid-test.
  globalSetup: './e2e/global-setup.ts',
  globalTeardown: './e2e/global-teardown.ts',
  use: {
    baseURL: 'http://localhost:5173',
    trace: 'retain-on-failure',
    // A fresh context is a first visit, and a first visit opens the guided
    // tour over the whole screen. Every spec but `tour.spec.ts` starts with
    // the tours already taken.
    storageState: TOURS_ALREADY_SEEN,
    // Registration requires the farm's GPS position, which the form reads from
    // the handset. Ilundo, so figures land in the seeded village.
    geolocation: { latitude: -8.1303, longitude: 35.1895 },
    permissions: ['geolocation'],
  },
  webServer: {
    // Tour positioning has failed only after bundling before. Exercise the
    // artifact Netlify serves rather than letting the development server give
    // production-only lifecycle bugs a pass.
    command: 'pnpm build && pnpm preview --host 127.0.0.1 --port 5173',
    url: 'http://localhost:5173',
    reuseExistingServer: !process.env.CI,
  },
})
