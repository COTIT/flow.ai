import { createFileRoute } from '@tanstack/react-router'

import { SurveysListScreen } from '@/features/farmer/SurveysListScreen'

export const Route = createFileRoute('/_farmer/farm/surveys/')({
  component: SurveysListScreen,
})
