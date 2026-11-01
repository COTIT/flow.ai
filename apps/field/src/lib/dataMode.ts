/**
 * Drives the demo banner. Never a database column — see CLAUDE.md.
 *
 * Kept apart from `@/lib/supabase` so reading the data mode never requires
 * the data client (and a test that mocks the client need not re-export it).
 */
export const isDemoData = import.meta.env.VITE_DATA_MODE === 'demo'
