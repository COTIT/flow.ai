import { useQuery } from '@tanstack/react-query'
import { farmSurveysEligibility } from '@genesy/api-client/endpoints/farm-surveys'
import type { Eligibility } from '@genesy/api-client/model'

import { workspaceId } from '@/app/workspace'
import { allPages } from '@/features/farmer/pages'
import { queryKeys } from '@/lib/queryKeys'

export type SurveyEligibility = Eligibility

/**
 * One row per survey the farmer can see: whether their household may answer
 * it, and if not, the API's reason — the same text a submit would refuse
 * with, so the list and the submit can never disagree. Nothing here
 * re-derives eligibility.
 */
export function fetchSurveyEligibility(): Promise<SurveyEligibility[]> {
  return allPages((params) => farmSurveysEligibility(workspaceId(), params))
}

export function useSurveyEligibility() {
  return useQuery({ queryKey: queryKeys.surveyEligibility(), queryFn: fetchSurveyEligibility })
}

/**
 * The Surveys tab badge: surveys this household may answer now. The count is
 * the API's answer; zero renders nothing.
 */
export function useSurveyBadgeCount(): number {
  const { data } = useSurveyEligibility()
  return (data ?? []).filter((row) => row.eligible).length
}
