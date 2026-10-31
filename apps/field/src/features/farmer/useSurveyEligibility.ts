import { useQuery } from '@tanstack/react-query'

import type { Database } from '@/lib/db.types'
import { queryKeys } from '@/lib/queryKeys'
import { supabase } from '@/lib/supabase'

type EligibilityRow = Database['public']['Functions']['app_survey_eligibility']['Returns'][number]

/**
 * The generated type cannot see nullability through RETURNS TABLE; these three
 * are NULL whenever the household has not answered, or may answer.
 */
export type SurveyEligibility = Omit<EligibilityRow, 'reason' | 'response_id' | 'voucher_id'> & {
  reason: string | null
  response_id: string | null
  voucher_id: string | null
}

/**
 * One row per survey the farmer can see: whether their household may answer
 * it, and if not, the database's reason. The farmer list and the submit RPC
 * share `survey_block_reason`, so the two can never disagree — nothing here
 * re-derives eligibility.
 */
export async function fetchSurveyEligibility(): Promise<SurveyEligibility[]> {
  const { data, error } = await supabase.rpc('app_survey_eligibility')
  if (error) throw new Error(error.message)
  return (data ?? []) as SurveyEligibility[]
}

export function useSurveyEligibility() {
  return useQuery({ queryKey: queryKeys.surveyEligibility(), queryFn: fetchSurveyEligibility })
}

/**
 * The Surveys tab badge: surveys this household may answer now. The count is
 * the database's answer; zero renders nothing.
 */
export function useSurveyBadgeCount(): number {
  const { data } = useSurveyEligibility()
  return (data ?? []).filter((row) => row.eligible).length
}
