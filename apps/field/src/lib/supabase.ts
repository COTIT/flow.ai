import { createClient } from '@supabase/supabase-js'

import { trailFetch } from '@/lib/bugTrail'
import type { Database } from '@/lib/db.types'

// db.types.ts is GENERATED. Never hand-edit it; regenerate after every
// migration change and commit the result (CLAUDE.md):
//   pnpm supabase gen types typescript --project-id <ref> > src/lib/db.types.ts

const url = import.meta.env.VITE_SUPABASE_URL
const anonKey = import.meta.env.VITE_SUPABASE_ANON_KEY

if (!url || !anonKey) {
  throw new Error(
    'VITE_SUPABASE_URL and VITE_SUPABASE_ANON_KEY are required. Copy .env.example to .env.',
  )
}

/**
 * The only data client in the app.
 *
 * Anon key only. A service-role key bypasses RLS, and RLS is the security
 * boundary — nothing in this app may hold one.
 */
export const supabase = createClient<Database>(url, anonKey, {
  auth: {
    persistSession: true,
    autoRefreshToken: true,
  },
  // Every API call lands on the bug-report trail (path and status only).
  global: { fetch: trailFetch },
})
