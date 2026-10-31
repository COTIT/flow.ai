import { createFileRoute } from '@tanstack/react-router'

import { RegisterScreen } from '@/features/officer/RegisterScreen'

export const Route = createFileRoute('/_officer/officer/register')({
  // The draft id lives in the URL so a reload finds the same draft. Held in
  // the URL rather than component state because component state does not
  // survive the interruption this screen exists to withstand.
  validateSearch: (search: Record<string, unknown>): { draft?: string } =>
    typeof search.draft === 'string' ? { draft: search.draft } : {},
  component: RegisterScreen,
})
