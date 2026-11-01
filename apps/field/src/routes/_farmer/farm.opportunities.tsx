import { createFileRoute } from '@tanstack/react-router'

import { FarmerOpportunitiesScreen } from '@/features/farmer/FarmerOpportunitiesScreen'

export const Route = createFileRoute('/_farmer/farm/opportunities')({
  component: FarmerOpportunitiesScreen,
})
