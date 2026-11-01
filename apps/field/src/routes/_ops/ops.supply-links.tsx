import { createFileRoute } from '@tanstack/react-router'

import { SupplyLinksScreen } from '@/features/ops/supply/SupplyLinksScreen'

export const Route = createFileRoute('/_ops/ops/supply-links')({
  component: SupplyLinksScreen,
})
