import { createFileRoute } from '@tanstack/react-router'

import { OfficerHomeScreen } from '@/features/officer/OfficerHomeScreen'

export const Route = createFileRoute('/_officer/officer/')({
  component: OfficerHomeScreen,
})
