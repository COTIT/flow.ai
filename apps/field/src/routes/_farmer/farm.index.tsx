import { createFileRoute } from '@tanstack/react-router'

import { FarmHomeScreen } from '@/features/farmer/FarmHomeScreen'

export const Route = createFileRoute('/_farmer/farm/')({
  component: FarmHomeScreen,
})
