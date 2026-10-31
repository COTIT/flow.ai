import { createFileRoute } from '@tanstack/react-router'

import { OpportunityDetailScreen } from '@/features/ops/OpportunityDetailScreen'

export const Route = createFileRoute('/_ops/ops/opportunities/$opportunityId')({
  component: OpportunityDetailScreen,
})
