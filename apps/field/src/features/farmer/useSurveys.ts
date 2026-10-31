import { useMutation, useQueries, useQuery, useQueryClient } from '@tanstack/react-query'

import { useSurveyEligibility, type SurveyEligibility } from '@/features/farmer/useSurveyEligibility'
import { useFarmerVouchers, type FarmerVoucher } from '@/features/farmer/useVouchers'
import type { Database } from '@/lib/db.types'
import { isUuid } from '@/lib/ids'
import { queryKeys } from '@/lib/queryKeys'
import { supabase } from '@/lib/supabase'

type Enums = Database['public']['Enums']
export type SurveyQuestionKind = Enums['survey_question_kind']

/** An option as authored. Swahili is null until a native reviewer supplies it. */
export interface SurveyOption {
  value: string
  label_en: string
  label_sw: string | null
}

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
 * `localisedField` picks at render.
 */
export interface FarmerSurvey {
  id: string
  title_en: string
  title_sw: string | null
  description_en: string | null
  description_sw: string | null
  reward_amount: number
  currency: string
  closes_at: string | null
  status: Enums['survey_status']
  questions: SurveyQuestion[]
}

const SURVEY_SELECT = `id, title_en, title_sw, description_en, description_sw,
  reward_amount, currency, closes_at, status,
  survey_question ( id, position, kind, prompt_en, prompt_sw, required, options )`

function parseOptions(raw: unknown): SurveyOption[] {
  if (!Array.isArray(raw)) return []
  return raw.flatMap((option) => {
    if (typeof option !== 'object' || option === null) return []
    const o = option as Record<string, unknown>
    if (typeof o.value !== 'string' || o.value === '') return []
    return [
      {
        value: o.value,
        label_en: typeof o.label_en === 'string' && o.label_en !== '' ? o.label_en : o.value,
        label_sw: typeof o.label_sw === 'string' && o.label_sw !== '' ? o.label_sw : null,
      },
    ]
  })
}

/**
 * One survey and its live questions, in position order.
 *
 * `survey_read` shows a farmer the live and closed surveys of their project
 * and village; `survey_question_read` follows it. Deleted questions are left
 * out on the server, so the form can never offer one the submit would refuse.
 */
export async function fetchSurvey(surveyId: string): Promise<FarmerSurvey | null> {
  // A malformed route param reaches the same "not found" as an id matching
  // nothing, rather than a uuid parse failure (QA #3).
  if (!isUuid(surveyId)) return null

  const { data, error } = await supabase
    .from('survey')
    .select(SURVEY_SELECT)
    .eq('id', surveyId)
    .is('survey_question.deleted_at', null)
    .order('position', { referencedTable: 'survey_question' })
    .maybeSingle()

  if (error) throw new Error(error.message)
  if (!data) return null

  const row = data as unknown as Record<string, unknown>
  const questions = ((row.survey_question ?? []) as Array<Record<string, unknown>>)
    .map((q) => ({
      id: q.id as string,
      position: Number(q.position),
      kind: q.kind as SurveyQuestionKind,
      prompt_en: q.prompt_en as string,
      prompt_sw: (q.prompt_sw as string | null) ?? null,
      required: Boolean(q.required),
      options: parseOptions(q.options),
    }))
    .sort((a, b) => a.position - b.position)

  return {
    id: row.id as string,
    title_en: row.title_en as string,
    title_sw: (row.title_sw as string | null) ?? null,
    description_en: (row.description_en as string | null) ?? null,
    description_sw: (row.description_sw as string | null) ?? null,
    // numeric(14,2): PostgREST may send it as text.
    reward_amount: Number(row.reward_amount),
    currency: row.currency as string,
    closes_at: (row.closes_at as string | null) ?? null,
    status: row.status as Enums['survey_status'],
    questions,
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
  /** The database's sentence for why this household cannot answer. */
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
 * database's, from the same `survey_block_reason` the submit runs. A survey
 * the RPC names but RLS does not show is left out — zero rows is an answer.
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

/** `p_answers`, keyed by question id. */
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

/** What `app_survey_submit` returns. `replayed` is a retry of an earlier save. */
export interface SurveySubmitResult {
  response_id: string
  voucher_id: string
  code: string
  amount: number
  currency: string
  expires_at: string
  replayed: boolean
}

/**
 * One call answers the survey and issues the voucher. `clientRef` makes it
 * idempotent: a retry after a timeout replays the first save's voucher
 * instead of being refused as a second answer.
 */
export async function submitSurvey(
  surveyId: string,
  answers: SurveyAnswers,
  clientRef: string,
): Promise<SurveySubmitResult> {
  const { data, error } = await supabase.rpc('app_survey_submit', {
    p_survey_id: surveyId,
    p_answers: answers,
    p_client_ref: clientRef,
  })
  // The refusal reasons are sentences written to be read; surfaced verbatim.
  if (error) throw new Error(error.message)
  return data as unknown as SurveySubmitResult
}

export function useSubmitSurvey(surveyId: string) {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ answers, clientRef }: { answers: SurveyAnswers; clientRef: string }) =>
      submitSurvey(surveyId, answers, clientRef),
    onSuccess: async (result) => {
      // The RPC has just returned the code; reading it back would be a
      // second round trip for the same ten characters.
      queryClient.setQueryData(queryKeys.voucherCode(result.voucher_id), result.code)
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: queryKeys.surveyEligibility() }),
        queryClient.invalidateQueries({ queryKey: queryKeys.farmerVouchers() }),
        queryClient.invalidateQueries({ queryKey: queryKeys.voucherTimeline(result.voucher_id) }),
      ])
    },
  })
}
