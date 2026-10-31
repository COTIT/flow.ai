import type { Chapter } from '@/app/tour/tourTypes'

/**
 * The ops tour, surveys — shared by ops and admin, with the authoring chapter
 * shown to admin only (ops read the same lists and results but cannot write).
 *
 * Every stop is explain-only unless it says `tryIt`; the tour opens rows but
 * never presses a button that writes. Approve, Reject, Publish and Void cannot
 * be undone, so they are explained here and never performed.
 *
 * Which survey the tour walks into matters. The list is ordered by creation
 * time, which the seed does not vary, so "the first row" could be the draft or
 * a live survey nobody has answered — whose voucher table is an empty state
 * that the stops after it would wait out. Rows carry `data-status` and
 * `data-answered` for exactly this (SurveyAdminListScreen). The selectors are
 * scoped by the table's test id rather than the row's, because a row test id is
 * passed as `rowTestId` and tourSteps.test.ts only reads `testId` and
 * `data-testid`.
 */
export const OPS_SURVEY_CHAPTERS: Chapter[] = [
  {
    id: 'surveys',
    titleKey: 'tour.ops.chapter.surveys',
    steps: [
      {
        route: '/ops/surveys',
        testId: 'survey-admin-list',
        titleKey: 'tour.ops.surveysTitle',
        bodyKey: 'tour.ops.surveysBody',
        placement: 'center',
      },
      {
        // Opens a live survey somebody has answered: it has vouchers to show,
        // and one in the "issued" state carries the cancel control.
        route: '/ops/surveys',
        testId: 'surveys-table',
        titleKey: 'tour.ops.surveysListTitle',
        bodyKey: 'tour.ops.surveysListBody',
        open: '[data-testid="surveys-table"] tr[data-status="live"][data-answered="true"]',
      },
      {
        route: '/ops/surveys/$surveyId',
        testId: 'survey-summary',
        titleKey: 'tour.ops.surveysSummaryTitle',
        bodyKey: 'tour.ops.surveysSummaryBody',
      },
      {
        route: '/ops/surveys/$surveyId',
        testId: 'survey-tally',
        titleKey: 'tour.ops.surveysTallyTitle',
        bodyKey: 'tour.ops.surveysTallyBody',
      },
      {
        // The spotlight is the table, so clicking a row is allowed through it;
        // the panel that opens below is not part of this stop, only its gate.
        route: '/ops/surveys/$surveyId',
        testId: 'vouchers-table',
        titleKey: 'tour.ops.surveysVouchersTitle',
        bodyKey: 'tour.ops.surveysVouchersBody',
        tryIt: true,
        gate: 'voucher-panel',
        optional: true,
      },
      {
        route: '/ops/surveys/$surveyId',
        testId: 'audit-timeline',
        titleKey: 'tour.ops.surveysTrailTitle',
        bodyKey: 'tour.ops.surveysTrailBody',
        optional: true,
      },
      {
        // Only an issued voucher has this control. The spotlight is the reason
        // box, not the Cancel button, so an explain-only stop cannot be
        // clicked through to the one-way action.
        route: '/ops/surveys/$surveyId',
        testId: 'voucher-void-reason',
        titleKey: 'tour.ops.surveysVoidTitle',
        bodyKey: 'tour.ops.surveysVoidBody',
        optional: true,
      },
    ],
  },
  {
    id: 'redemptions',
    titleKey: 'tour.ops.chapter.redemptions',
    steps: [
      {
        route: '/ops/surveys/redemptions',
        testId: 'redemptions',
        titleKey: 'tour.ops.redemptionsTitle',
        bodyKey: 'tour.ops.redemptionsBody',
        placement: 'center',
      },
      {
        route: '/ops/surveys/redemptions',
        testId: 'redemption-totals-table',
        titleKey: 'tour.ops.redemptionsTotalsTitle',
        bodyKey: 'tour.ops.redemptionsTotalsBody',
        optional: true,
      },
      {
        route: '/ops/surveys/redemptions',
        testId: 'redemptions-table',
        titleKey: 'tour.ops.redemptionsLogTitle',
        bodyKey: 'tour.ops.redemptionsLogBody',
        optional: true,
      },
      {
        // Last on purpose: changing the dates can empty both tables above.
        route: '/ops/surveys/redemptions',
        testId: 'redemptions-range',
        titleKey: 'tour.ops.redemptionsRangeTitle',
        bodyKey: 'tour.ops.redemptionsRangeBody',
        tryIt: true,
      },
    ],
  },
  {
    id: 'authoring',
    titleKey: 'tour.ops.chapter.authoring',
    audience: 'admin',
    steps: [
      {
        // Spotlights the button alone: the form it opens sits outside the
        // cut-out, so the person can look at it but not type into it, and
        // nothing can be saved from here.
        route: '/ops/surveys',
        testId: 'survey-create-open',
        titleKey: 'tour.ops.authoringCreateTitle',
        bodyKey: 'tour.ops.authoringCreateBody',
        tryIt: true,
      },
      {
        route: '/ops/surveys',
        testId: 'surveys-table',
        titleKey: 'tour.ops.authoringDraftTitle',
        bodyKey: 'tour.ops.authoringDraftBody',
        open: '[data-testid="surveys-table"] tr[data-status="draft"]',
      },
      {
        route: '/ops/surveys/$surveyId',
        testId: 'survey-editor',
        titleKey: 'tour.ops.authoringEditorTitle',
        bodyKey: 'tour.ops.authoringEditorBody',
      },
      {
        route: '/ops/surveys/$surveyId',
        testId: 'questions-editor',
        titleKey: 'tour.ops.authoringQuestionsTitle',
        bodyKey: 'tour.ops.authoringQuestionsBody',
      },
      {
        route: '/ops/surveys/$surveyId',
        testId: 'survey-publish',
        titleKey: 'tour.ops.authoringPublishTitle',
        bodyKey: 'tour.ops.authoringPublishBody',
        optional: true,
      },
    ],
  },
]
