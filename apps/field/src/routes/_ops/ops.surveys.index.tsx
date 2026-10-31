import { createFileRoute } from '@tanstack/react-router'

import { SurveyAdminListScreen } from '@/features/ops/surveys/SurveyAdminListScreen'

export const Route = createFileRoute('/_ops/ops/surveys/')({
  component: SurveyAdminListScreen,
})
