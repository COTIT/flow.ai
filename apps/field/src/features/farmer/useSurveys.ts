import { useMutation, useQueries, useQuery, useQueryClient } from '@tanstack/react-query'
import { farmSurveysAnswer, farmSurveysGet } from '@genesy/api-client/endpoints/farm-surveys'
import type { Money, QuestionOption, Submitted } from '@genesy/api-client/model'

import { workspaceId } from '@/app/workspace'
import { useSurveyEligibility, type SurveyEligibility } from '@/features/farmer/useSurveyEligibility'
import { useFarmerVouchers, type FarmerVoucher } from '@/features/farmer/useVouchers'
import { apiStatus } from '@/lib/apiError'
import { isUuid } from '@/lib/ids'
import { queryKeys } from '@/lib/queryKeys'

export type SurveyQuestionKind = 'single_choice' | 'multi_choice' | 'number' | 'text' | 'yes_no'

/** An option as authored. Swahili is null until a native reviewer supplies it. */
export type SurveyOption = QuestionOption

export interface SurveyQuestion {
  id: string
  position: number
  kind: SurveyQuestionKind
  prompt_en: string
  prompt_sw: string | null
  required: boolean
  options: SurveyOption[]
}

/**
 * One survey as a farmer answers it: both languages cached, neither chosen —
 * `localisedField` picks at render. The reward is the API's Money.
 */
export interface FarmerSurvey {
  id: string
  title_en: string
  title_sw: string | null
  description_en: string | null
  description_sw: string | null
  reward: Money
  closes_at: string | null
  status: string
  questions: SurveyQuestion[]
}

/**
 * One survey and its live questions, in position order (the API orders them
 * and leaves deleted ones out). `null` when the API says 404: not visible to
 * this household, or not a survey.
 */
export async function fetchSurvey(surveyId: string): Promise<FarmerSurvey | null> {
  // A malformed route param reaches the same "not found" as an id matching nothing (QA #3).
  if (!isUuid(surveyId)) return null
  try {
    const { data } = await farmSurveysGet(workspaceId(), surveyId)
    return {
      id: data.id,
      title_en: data.title_en,
      title_sw: data.title_sw,
      description_en: data.description_en,
      description_sw: data.description_sw,
      reward: data.reward,
      closes_at: data.closes_at,
      status: data.status,
      questions: data.questions.map((q) => ({ ...q, kind: q.kind as SurveyQuestionKind })),
    }
  } catch (error) {
    if (apiStatus(error) === 404) return null
    throw error
  }
}

export function useSurvey(surveyId: string) {
  return useQuery({ queryKey: queryKeys.survey(surveyId), queryFn: () => fetchSurvey(surveyId) })
}

/** How a survey reads on the list. */
export type SurveyCardState = 'new' | 'answered' | 'unavailable'

export interface SurveyListRow {
  survey: FarmerSurvey
  state: SurveyCardState
  /** The API's sentence for why this household cannot answer. */
  reason: string | null
  voucherId: string | null
  voucher: FarmerVoucher | null
}

const RANK: Record<SurveyCardState, number> = { new: 0, answered: 1, unavailable: 2 }

function stateOf(row: SurveyEligibility): SurveyCardState {
  if (row.eligible) return 'new'
  if (row.response_id || row.voucher_id) return 'answered'
  return 'unavailable'
}

function closesAt(survey: FarmerSurvey): number {
  return survey.closes_at ? Date.parse(survey.closes_at) : Number.POSITIVE_INFINITY
}

/**
 * Eligibility joined to the survey rows and the household's vouchers.
 *
 * Nothing here decides who may answer: `eligible` and `reason` are the
 * API's, the same text a submit would refuse with. A survey
 * the eligibility list names but GET answers 404 for is left out — zero rows is an answer.
 *
 * Order: surveys to answer now, then answered ones, then the rest; within
 * each, the one closing soonest first.
 */
export function buildSurveyList(
  eligibility: SurveyEligibility[],
  surveys: Map<string, FarmerSurvey | null>,
  vouchers: FarmerVoucher[],
): SurveyListRow[] {
  const voucherById = new Map(vouchers.map((voucher) => [voucher.id, voucher]))

  return eligibility
    .flatMap((row) => {
      const survey = surveys.get(row.survey_id)
      if (!survey) return []
      return [
        {
          survey,
          state: stateOf(row),
          reason: row.reason,
          voucherId: row.voucher_id,
          voucher: row.voucher_id ? (voucherById.get(row.voucher_id) ?? null) : null,
        },
      ]
    })
    .sort(
      (a, b) =>
        RANK[a.state] - RANK[b.state] ||
        closesAt(a.survey) - closesAt(b.survey) ||
        a.survey.title_en.localeCompare(b.survey.title_en),
    )
}

/**
 * The farmer's survey list: eligibility, each survey (cached under
 * `survey(id)`, so opening one is instant), and the household's vouchers for
 * the status of each answered survey.
 */
export function useSurveyList() {
  const eligibility = useSurveyEligibility()
  const rows = eligibility.data ?? []
  const surveys = useQueries({
    queries: rows.map((row) => ({
      queryKey: queryKeys.survey(row.survey_id),
      queryFn: () => fetchSurvey(row.survey_id),
    })),
  })
  const vouchers = useFarmerVouchers()

  const isLoading =
    eligibility.isLoading || vouchers.isLoading || surveys.some((query) => query.isLoading)
  const error: Error | null =
    eligibility.error ?? surveys.find((query) => query.error)?.error ?? vouchers.error ?? null

  const data =
    isLoading || error
      ? undefined
      : buildSurveyList(
          rows,
          new Map(rows.map((row, index) => [row.survey_id, surveys[index]?.data ?? null])),
          vouchers.data ?? [],
        )

  const refetch = () => {
    void eligibility.refetch()
    void vouchers.refetch()
    for (const query of surveys) void query.refetch()
  }

  return { data, isLoading, error, refetch }
}

/**
 * The database's refusal reasons are plain lowercase English sentences, and
 * they are shown as written. Only the first letter is raised, for display.
 */
export function sentence(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1)
}

/**
 * A multi-choice answer in the string-valued draft: a JSON array, because an
 * option value may itself contain a comma.
 */
export function writeChoices(values: string[]): string {
  return JSON.stringify(values)
}

export function readChoices(raw: string | undefined): string[] {
  if (!raw) return []
  try {
    const parsed: unknown = JSON.parse(raw)
    return Array.isArray(parsed) ? parsed.filter((v): v is string => typeof v === 'string') : []
  } catch {
    return []
  }
}

/** `answers`, keyed by question id. */
export type SurveyAnswers = Record<string, string | string[] | boolean>

/**
 * The draft's strings, in the JSON shape each kind takes.
 *
 * Unanswered questions are omitted — required or not. Whether a required
 * question may be skipped, an option is listed or "12,5" is a number is the
 * database's decision, and its refusal names the question; a copy of those
 * rules here would drift from the one that counts.
 */
export function toSurveyAnswers(
  questions: SurveyQuestion[],
  values: Record<string, string | undefined>,
): SurveyAnswers {
  const answers: SurveyAnswers = {}
  for (const question of questions) {
    const raw = values[question.id] ?? ''
    switch (question.kind) {
      case 'multi_choice': {
        const chosen = readChoices(raw)
        if (chosen.length > 0) answers[question.id] = chosen
        break
      }
      case 'yes_no':
        if (raw === 'yes') answers[question.id] = true
        else if (raw === 'no') answers[question.id] = false
        break
      default:
        // single_choice, number, text: sent as typed. The database trims and
        // parses; blank is unanswered.
        if (raw.trim() !== '') answers[question.id] = raw
    }
  }
  return answers
}

/** `replayed`: a retry of an earlier save with the same `client_ref`; the same voucher comes back. */
export type SurveySubmitResult = Submitted

/**
 * One call answers the survey and issues the voucher. `clientRef` makes it
 * idempotent: a retry after a timeout replays the first save's voucher
 * instead of being refused as a second answer. A refusal rejects with the
 * API error; its sentence is shown as written.
 */
export async function submitSurvey(
  surveyId: string,
  answers: SurveyAnswers,
  clientRef: string,
): Promise<SurveySubmitResult> {
  return (await farmSurveysAnswer(workspaceId(), surveyId, { answers, client_ref: clientRef })).data
}

export function useSubmitSurvey(surveyId: string) {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ answers, clientRef }: { answers: SurveyAnswers; clientRef: string }) =>
      submitSurvey(surveyId, answers, clientRef),
    onSuccess: async (result) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: queryKeys.surveyEligibility() }),
        queryClient.invalidateQueries({ queryKey: queryKeys.farmerVouchers() }),
        queryClient.invalidateQueries({ queryKey: queryKeys.voucherTimeline(result.voucher_id) }),
      ])
    },
  })
}
