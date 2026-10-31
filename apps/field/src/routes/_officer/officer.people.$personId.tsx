import { createFileRoute } from '@tanstack/react-router'

import { PersonDetailScreen } from '@/features/officer/PersonDetailScreen'

export const Route = createFileRoute('/_officer/officer/people/$personId')({
  component: PersonDetailScreen,
})
