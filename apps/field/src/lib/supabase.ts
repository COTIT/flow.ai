import { createClient } from '@supabase/supabase-js'

import { trailFetch } from '@/lib/bugTrail'
import type { Database } from '@/lib/db.types'

// db.types.ts is GENERATED. Never hand-edit it; regenerate after every
// migration change and commit the result (CLAUDE.md):
//   pnpm supabase gen types typescript --project-id <ref> > src/lib/db.types.ts

// shortcut: during the Genesy port the client must load without Supabase settings (screens not yet
// ported are hidden, see @/lib/ported); an unset value points at a dead local port. Deleted in port step 8.
const url = import.meta.env.VITE_SUPABASE_URL || 'http://127.0.0.1:9'
const anonKey = import.meta.env.VITE_SUPABASE_ANON_KEY || 'not-configured'

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
