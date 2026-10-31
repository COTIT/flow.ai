import { useMutation, useQuery, useQueryClient, type QueryClient } from '@tanstack/react-query'

import i18n from '@/i18n'
import type { AuditEvent } from '@/components/AuditTimeline'
import type { Database, Json } from '@/lib/db.types'
import { isUuid, newUuid } from '@/lib/ids'
import { queryKeys } from '@/lib/queryKeys'
import { supabase } from '@/lib/supabase'
import type { QuestionKind, StoredOption } from '@/features/ops/surveys/questionOptions'

type Tables = Database['public']['Tables']
type Views = Database['public']['Views']
type Functions = Database['public']['Functions']

export type Survey = Tables['survey']['Row']
export type SurveyStatus = Survey['status']
export type SurveyQuestion = Tables['survey_question']['Row']
export type SurveySummary = Views['v_survey_summary']['Row']
export type TallyRow = Pick<Views['v_survey_answer_tally']['Row'], 'question_id' | 'option_value' | 'answer_count'>
export type NumberSummaryRow = Pick<
  Views['v_survey_number_summary']['Row'],
  'question_id' | 'answer_count' | 'average' | 'minimum' | 'maximum'
>
export type SurveyVoucher = Functions['app_survey_vouchers']['Returns'][number]
export type RedemptionLogRow = Functions['app_redemption_log']['Returns'][number]
export type RedemptionTotalRow = Functions['app_redemption_totals']['Returns'][number]

/**
 * Surveys, results and vouchers for ops and admin.
 *
 * Every figure on these screens is the database's: v_survey_summary counts
 * responses and vouchers, the tally views count answers, and the redemption
 * RPCs total the cash. Nothing here sums or counts rows (business-rules §11).
 *
 * Titles, prompts and option labels arrive in BOTH languages and are chosen
 * at render — a queryFn never knows the language (localisation.test.ts).
 */

const LIST_SELECT =
  'id, project_id, village_id, title_en, title_sw, reward_amount, currency, status, closes_at, published_at, created_at'

export type SurveyListRow = Pick<
  Survey,
  | 'id'
  | 'project_id'
  | 'village_id'
  | 'title_en'
  | 'title_sw'
  | 'reward_amount'
  | 'currency'
  | 'status'
  | 'closes_at'
  | 'published_at'
  | 'created_at'
> & { summary: SurveySummary | null }

export async function fetchSurveyAdminList(): Promise<SurveyListRow[]> {
  const [surveys, summaries] = await Promise.all([
    supabase.from('survey').select(LIST_SELECT).order('created_at', { ascending: false }),
    supabase.from('v_survey_summary').select('*'),
  ])
  if (surveys.error) throw new Error(surveys.error.message)
  if (summaries.error) throw new Error(summaries.error.message)

  // A pairing by id, not an aggregate: each figure is the view's own.
  const bySurvey = new Map((summaries.data ?? []).map((s) => [s.survey_id, s]))
  return ((surveys.data ?? []) as SurveyListRow[]).map((s) => ({
    ...s,
    summary: bySurvey.get(s.id) ?? null,
  }))
}

export function useSurveyAdminList() {
  return useQuery({ queryKey: queryKeys.surveyAdmin(), queryFn: fetchSurveyAdminList })
}

export interface SurveyAdminDetail {
  survey: Survey
  questions: SurveyQuestion[]
  summary: SurveySummary | null
}

export async function fetchSurveyAdminDetail(surveyId: string): Promise<SurveyAdminDetail | null> {
  // QA #3: a malformed route param reaches "not found", not a uuid parse error.
  if (!isUuid(surveyId)) return null

  const [survey, questions, summary] = await Promise.all([
    supabase.from('survey').select('*').eq('id', surveyId).maybeSingle(),
    supabase
      .from('survey_question')
      .select('*')
      .eq('survey_id', surveyId)
      .is('deleted_at', null)
      .order('position'),
    supabase.from('v_survey_summary').select('*').eq('survey_id', surveyId).maybeSingle(),
  ])
  if (survey.error) throw new Error(survey.error.message)
  if (questions.error) throw new Error(questions.error.message)
  if (summary.error) throw new Error(summary.error.message)

  // Zero rows is an answer: RLS says this survey is not visible.
  if (!survey.data) return null
  return {
    survey: survey.data as Survey,
    questions: (questions.data ?? []) as SurveyQuestion[],
    summary: (summary.data as SurveySummary | null) ?? null,
  }
}

export function useSurveyAdminDetail(surveyId: string) {
  return useQuery({
    queryKey: queryKeys.surveyAdminDetail(surveyId),
    queryFn: () => fetchSurveyAdminDetail(surveyId),
  })
}

export interface SurveyTally {
  choices: TallyRow[]
  numbers: NumberSummaryRow[]
}

export async function fetchSurveyTally(surveyId: string): Promise<SurveyTally> {
  const [choices, numbers] = await Promise.all([
    supabase
      .from('v_survey_answer_tally')
      .select('question_id, option_value, answer_count')
      .eq('survey_id', surveyId),
    supabase
      .from('v_survey_number_summary')
      .select('question_id, answer_count, average, minimum, maximum')
      .eq('survey_id', surveyId),
  ])
  if (choices.error) throw new Error(choices.error.message)
  if (numbers.error) throw new Error(numbers.error.message)
  return { choices: choices.data ?? [], numbers: numbers.data ?? [] }
}

export function useSurveyTally(surveyId: string, enabled = true) {
  return useQuery({
    queryKey: queryKeys.surveyTally(surveyId),
    enabled,
    queryFn: () => fetchSurveyTally(surveyId),
  })
}

export function useSurveyVouchers(surveyId: string, enabled = true) {
  return useQuery({
    queryKey: queryKeys.surveyVouchers(surveyId),
    enabled,
    queryFn: async () => {
      const { data, error } = await supabase.rpc('app_survey_vouchers', { p_survey_id: surveyId })
      if (error) throw new Error(error.message)
      return (data ?? []) as SurveyVoucher[]
    },
  })
}

/** The full trail for one voucher; the database filters it by who asks. */
export function useVoucherTimeline(voucherId: string | null) {
  return useQuery({
    queryKey: queryKeys.voucherTimeline(voucherId ?? ''),
    enabled: Boolean(voucherId),
    queryFn: async () => {
      const { data, error } = await supabase.rpc('app_voucher_timeline', { p_voucher_id: voucherId! })
      if (error) throw new Error(error.message)
      return (data ?? []) as unknown as AuditEvent[]
    },
  })
}

export interface RedemptionReport {
  totals: RedemptionTotalRow[]
  log: RedemptionLogRow[]
}

/** Both reads for one Tanzanian date range, cached together. */
export async function fetchRedemptions(from: string, to: string): Promise<RedemptionReport> {
  const [totals, log] = await Promise.all([
    supabase.rpc('app_redemption_totals', { p_from: from, p_to: to }),
    supabase.rpc('app_redemption_log', { p_from: from, p_to: to }),
  ])
  if (totals.error) throw new Error(totals.error.message)
  if (log.error) throw new Error(log.error.message)
  return { totals: totals.data ?? [], log: log.data ?? [] }
}

export function useRedemptions(from: string, to: string) {
  return useQuery({
    queryKey: queryKeys.redemptionLog(from, to),
    queryFn: () => fetchRedemptions(from, to),
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

/**
 * RLS drops an update it refuses to zero rows, with no error at all. The
 * controls exist only for admins, so reaching this means the screen offered
 * something it should not have — and it is still not a save.
 */
function requireRow(data: unknown[] | null) {
  if (!data || data.length === 0) throw new Error(i18n.t('error.notAllowed'))
}

export interface NewSurvey {
  id: string
  project_id: string
  title_en: string
  /** Null is sent as null: the not-null constraint answers it, not the form. */
  reward_amount: number | null
}

/**
 * A new survey is a draft. created_by, created_at and the publish columns are
 * stamped by survey_guard and never sent.
 */
export function useCreateSurvey() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (input: NewSurvey) => {
      const { data, error } = await supabase
        .from('survey')
        .insert({
          id: input.id,
          project_id: input.project_id,
          title_en: input.title_en,
          reward_amount: input.reward_amount as number,
        })
        .select('id')
        .single()
      if (error) throw new Error(error.message)
      return data
    },
    onSuccess: () => invalidateSurvey(queryClient),
  })
}

/**
 * The columns an admin may change on a draft. Null is allowed for every one
 * of them: a blank form field is sent as null and the not-null constraint,
 * not the form, answers it.
 */
export type SurveyPatch = {
  [K in
    | 'title_en'
    | 'title_sw'
    | 'description_en'
    | 'description_sw'
    | 'reward_amount'
    | 'max_households'
    | 'audit_rate'
    | 'closes_at']?: Survey[K] | null
}

export function useUpdateSurvey(surveyId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (patch: SurveyPatch) => {
      const { data, error } = await supabase
        .from('survey')
        .update(patch as Tables['survey']['Update'])
        .eq('id', surveyId)
        .select('id')
      if (error) throw new Error(error.message)
      requireRow(data)
    },
    onSuccess: () => invalidateSurvey(queryClient, surveyId),
  })
}

/**
 * Publish (draft -> live) and close (live -> closed). survey_guard owns the
 * status machine and every precondition of going live; its message is the
 * error.
 */
export function useSetSurveyStatus(surveyId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (status: Extract<SurveyStatus, 'live' | 'closed'>) => {
      const { data, error } = await supabase.from('survey').update({ status }).eq('id', surveyId).select('id')
      if (error) throw new Error(error.message)
      requireRow(data)
    },
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

export function useSaveQuestion(surveyId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (input: QuestionInput) => {
      const fields = {
        kind: input.kind,
        prompt_en: input.prompt_en.trim(),
        prompt_sw: input.prompt_sw.trim() || null,
        required: input.required,
        options: input.options as Json,
      }
      if (input.id) {
        const { data, error } = await supabase
          .from('survey_question')
          .update(fields)
          .eq('id', input.id)
          .select('id')
        if (error) throw new Error(error.message)
        requireRow(data)
        return { id: input.id }
      }
      const { data, error } = await supabase
        .from('survey_question')
        .insert({ id: newUuid(), survey_id: surveyId, position: input.position, ...fields })
        .select('id')
        .single()
      if (error) throw new Error(error.message)
      return data
    },
    onSuccess: () => invalidateSurvey(queryClient, surveyId),
  })
}

/** There are no DELETE policies: a question is removed by stamping it. */
export function useRemoveQuestion(surveyId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (questionId: string) => {
      const { data, error } = await supabase
        .from('survey_question')
        .update({ deleted_at: new Date().toISOString() })
        .eq('id', questionId)
        .select('id')
      if (error) throw new Error(error.message)
      requireRow(data)
    },
    onSuccess: () => invalidateSurvey(queryClient, surveyId),
  })
}

interface Placed {
  id: string
  position: number
}

/** Move up / down: two questions trade positions. */
export function useSwapQuestions(surveyId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ first, second }: { first: Placed; second: Placed }) => {
      for (const [row, position] of [
        [first, second.position],
        [second, first.position],
      ] as const) {
        const { data, error } = await supabase
          .from('survey_question')
          .update({ position })
          .eq('id', row.id)
          .select('id')
        if (error) throw new Error(error.message)
        requireRow(data)
      }
    },
    onSettled: () => invalidateSurvey(queryClient, surveyId),
  })
}

/** app_voucher_void: issued vouchers only, a reason required — both the RPC's rules. */
export function useVoidVoucher(surveyId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ voucherId, reason }: { voucherId: string; reason: string }) => {
      const { error } = await supabase.rpc('app_voucher_void', {
        p_voucher_id: voucherId,
        p_reason: reason,
      })
      if (error) throw new Error(error.message)
    },
    onSuccess: async (_data, { voucherId }) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: queryKeys.surveyVouchers(surveyId) }),
        queryClient.invalidateQueries({ queryKey: queryKeys.surveyAdmin() }),
        queryClient.invalidateQueries({ queryKey: queryKeys.voucherTimeline(voucherId) }),
        queryClient.invalidateQueries({ queryKey: ['redemptionLog'] }),
      ])
    },
  })
}
