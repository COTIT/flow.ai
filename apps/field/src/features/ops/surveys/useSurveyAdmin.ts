import { useMutation, useQuery, useQueryClient, type QueryClient } from '@tanstack/react-query'
import { currenciesList } from '@genesy/api-client/endpoints/currencies'
import {
  farmSurveysAddQuestion,
  farmSurveysClose,
  farmSurveysCreate,
  farmSurveysGet,
  farmSurveysList,
  farmSurveysPublish,
  farmSurveysRemoveQuestion,
  farmSurveysReplaceQuestion,
  farmSurveysResults,
  farmSurveysUpdate,
  farmSurveysVouchers,
} from '@genesy/api-client/endpoints/farm-surveys'
import { farmVouchersVoid } from '@genesy/api-client/endpoints/farm-vouchers'
import type {
  CurrencyResource,
  QuestionRequest,
  QuestionResource,
  SurveyRequest,
  SurveyResource,
  SurveyResults,
  SurveyVoucher as SurveyVoucherRow,
} from '@genesy/api-client/model'

import { workspaceId } from '@/app/workspace'
import { allPages } from '@/features/farmer/pages'
import type { QuestionKind, StoredOption } from '@/features/ops/surveys/questionOptions'
import { apiStatus } from '@/lib/apiError'
import { isUuid } from '@/lib/ids'
import { queryKeys } from '@/lib/queryKeys'

// The API documents these statuses as strings; they are narrowed here for the status pills.
export type SurveyStatus = 'draft' | 'live' | 'closed'
export type Survey = Omit<SurveyResource, 'status'> & { status: SurveyStatus }
export type SurveyQuestion = QuestionResource
export type SurveyListRow = Survey
export type SurveyResultsData = SurveyResults
export type SurveyVoucher = Omit<SurveyVoucherRow, 'status'> & { status: 'issued' | 'redeemed' | 'void' }
/** A survey body as the forms send it: a blank or unreadable reward goes as null, for the API to refuse. */
export type SurveyBody = Omit<SurveyRequest, 'reward_minor'> & { reward_minor?: number | null }
export type { NumberSummary as NumberSummaryRow, TallyRow } from '@genesy/api-client/model'

/**
 * Surveys, results and vouchers for ops and admin, on the Genesy API.
 *
 * Every figure on these screens is the API's: GET .../results counts
 * responses and vouchers and sums their cash. Nothing here sums or counts
 * rows (business-rules §11). Titles, prompts and option labels arrive in BOTH
 * languages and are chosen at render.
 */

export async function fetchSurveyAdminList(): Promise<SurveyListRow[]> {
  const workspace = workspaceId()
  return (await allPages((params) => farmSurveysList(workspace, params))) as Survey[]
}

export function useSurveyAdminList() {
  return useQuery({ queryKey: queryKeys.surveyAdmin(), queryFn: fetchSurveyAdminList })
}

export interface SurveyAdminDetail {
  survey: Survey
  questions: SurveyQuestion[]
}

export async function fetchSurveyAdminDetail(surveyId: string): Promise<SurveyAdminDetail | null> {
  // QA #3: a malformed route param reaches "not found", not a 422.
  if (!isUuid(surveyId)) return null
  try {
    const survey = (await farmSurveysGet(workspaceId(), surveyId)).data as Survey
    return { survey, questions: [...survey.questions].sort((a, b) => a.position - b.position) }
  } catch (error) {
    // A 404 is an answer: this survey is not visible to this user.
    if (apiStatus(error) === 404) return null
    throw error
  }
}

export function useSurveyAdminDetail(surveyId: string) {
  return useQuery({
    queryKey: queryKeys.surveyAdminDetail(surveyId),
    queryFn: () => fetchSurveyAdminDetail(surveyId),
  })
}

/** Counts, cash sums, the option tally and number summaries — all the API's. */
export function useSurveyResults(surveyId: string, enabled = true) {
  return useQuery({
    queryKey: queryKeys.surveyTally(surveyId),
    enabled,
    queryFn: async () => (await farmSurveysResults(workspaceId(), surveyId)).data,
  })
}

/** Every voucher of one survey; the API sends no codes here. */
export function useSurveyVouchers(surveyId: string, enabled = true) {
  return useQuery({
    queryKey: queryKeys.surveyVouchers(surveyId),
    enabled,
    queryFn: async () => {
      const workspace = workspaceId()
      return (await allPages((params) => farmSurveysVouchers(workspace, surveyId, params))) as SurveyVoucher[]
    },
  })
}

/** The workspace's active currencies: a new survey's reward is in one of them. */
export function useCurrencies() {
  return useQuery({
    queryKey: ['currencies'] as const,
    queryFn: async (): Promise<CurrencyResource[]> => {
      const workspace = workspaceId()
      return (await allPages((params) => currenciesList(workspace, params))).filter((c) => c.active)
    },
  })
}

// ── writes ──────────────────────────────────────────────────

/** The invalidation map's "survey write / publish" row. */
async function invalidateSurvey(queryClient: QueryClient, surveyId?: string) {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: queryKeys.surveyAdmin() }),
    surveyId
      ? queryClient.invalidateQueries({ queryKey: queryKeys.surveyAdminDetail(surveyId) })
      : Promise.resolve(),
    queryClient.invalidateQueries({ queryKey: queryKeys.surveyEligibility() }),
  ])
}

/** A new survey is a draft. The body's client `id` makes a retry return the same survey (200). */
export function useCreateSurvey() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (input: SurveyBody) => (await farmSurveysCreate(workspaceId(), input as SurveyRequest)).data,
    onSuccess: () => invalidateSurvey(queryClient),
  })
}

export function useUpdateSurvey(surveyId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (patch: SurveyBody) =>
      (await farmSurveysUpdate(workspaceId(), surveyId, patch as SurveyRequest)).data,
    onSuccess: () => invalidateSurvey(queryClient, surveyId),
  })
}

/**
 * Publish (draft -> live) and close (live -> closed). The API owns the status
 * machine and every precondition of going live; its message is the error.
 */
export function useSetSurveyStatus(surveyId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (status: Extract<SurveyStatus, 'live' | 'closed'>) =>
      (status === 'live' ? farmSurveysPublish : farmSurveysClose)(workspaceId(), surveyId),
    onSuccess: () => invalidateSurvey(queryClient, surveyId),
  })
}

export interface QuestionInput {
  id?: string
  position: number
  kind: QuestionKind
  prompt_en: string
  prompt_sw: string
  required: boolean
  options: StoredOption[]
}

function questionBody(input: Omit<QuestionInput, 'id'> | SurveyQuestion, position = input.position): QuestionRequest {
  return {
    position,
    kind: input.kind,
    prompt_en: input.prompt_en.trim(),
    prompt_sw: input.prompt_sw?.trim() || null,
    required: input.required,
    options: input.options as QuestionRequest['options'],
  }
}

export function useSaveQuestion(surveyId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ id, ...input }: QuestionInput) => {
      const workspace = workspaceId()
      if (id) {
        await farmSurveysReplaceQuestion(workspace, surveyId, id, questionBody(input))
        return { id }
      }
      return (await farmSurveysAddQuestion(workspace, surveyId, questionBody(input))).data
    },
    onSuccess: () => invalidateSurvey(queryClient, surveyId),
  })
}

export function useRemoveQuestion(surveyId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (questionId: string) => farmSurveysRemoveQuestion(workspaceId(), surveyId, questionId),
    onSuccess: () => invalidateSurvey(queryClient, surveyId),
  })
}

/** Move up / down: two questions trade positions, each replaced whole (PUT). */
export function useSwapQuestions(surveyId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ first, second }: { first: SurveyQuestion; second: SurveyQuestion }) => {
      const workspace = workspaceId()
      await farmSurveysReplaceQuestion(workspace, surveyId, first.id, questionBody(first, second.position))
      await farmSurveysReplaceQuestion(workspace, surveyId, second.id, questionBody(second, first.position))
    },
    onSettled: () => invalidateSurvey(queryClient, surveyId),
  })
}

/** Unredeemed vouchers only, a reason required, ops/admin only — all the API's rules. */
export function useVoidVoucher(surveyId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ voucherId, reason }: { voucherId: string; reason: string }) =>
      farmVouchersVoid(workspaceId(), voucherId, { reason }),
    onSuccess: async (_data, { voucherId }) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: queryKeys.surveyVouchers(surveyId) }),
        queryClient.invalidateQueries({ queryKey: queryKeys.surveyTally(surveyId) }),
        queryClient.invalidateQueries({ queryKey: queryKeys.surveyAdmin() }),
        queryClient.invalidateQueries({ queryKey: queryKeys.voucherTimeline(voucherId) }),
        queryClient.invalidateQueries({ queryKey: ['redemptionLog'] }),
      ])
    },
  })
}
