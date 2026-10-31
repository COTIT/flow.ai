import { createFileRoute } from '@tanstack/react-router'

import { RedemptionsScreen } from '@/features/ops/surveys/RedemptionsScreen'
import { validateRedemptionSearch } from '@/features/ops/surveys/surveyDates'

export const Route = createFileRoute('/_ops/ops/surveys/redemptions')({
  // The reconciliation range lives in the URL as validated YYYY-MM-DD dates,
  // so a stale bookmark degrades to the default range instead of breaking.
  validateSearch: validateRedemptionSearch,
  component: RedemptionsScreen,
})
