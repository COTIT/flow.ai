import { createFileRoute } from '@tanstack/react-router'

import { SurveyScreen } from '@/features/farmer/SurveyScreen'

export const Route = createFileRoute('/_farmer/farm/surveys/$surveyId')({
  component: SurveyScreen,
})
