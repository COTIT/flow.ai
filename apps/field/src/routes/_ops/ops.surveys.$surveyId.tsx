import { createFileRoute } from '@tanstack/react-router'

import { SurveyAdminDetailScreen } from '@/features/ops/surveys/SurveyAdminDetailScreen'

export const Route = createFileRoute('/_ops/ops/surveys/$surveyId')({
  component: SurveyAdminDetailScreen,
})
