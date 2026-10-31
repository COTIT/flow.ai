import type { Chapter } from '@/app/tour/tourTypes'

/**
 * The ops tour, first part — shared by ops and admin: the welcome, the work
 * queues, requests, buyer demand and the reference lists, in sidebar order.
 *
 * Every stop is explain-only unless it says `tryIt`; the tour opens rows but
 * never presses a button that writes. Approve, Reject, attaching supply and
 * declining an opportunity cannot be undone, so they are explained here and
 * never performed.
 */

/**
 * The request the review chapter opens.
 *
 * Prefers the one that is under review — it is the only status that shows the
 * decision note and the Approve / Reject buttons, which is what the chapter
 * explains. When the table has no such row (a filter is on, or the request was
 * decided) it falls back to the first row, so a chosen filter never costs the
 * person the review screens. Opening is navigation and nothing else.
 *
 * The status pill carries `data-status`; `:has()` is available in every browser
 * the app supports.
 */
const REQUEST_TO_REVIEW =
  '[data-testid="requests-table"]:has([data-status="under_review"]) tr:has([data-status="under_review"]), ' +
  '[data-testid="requests-table"]:not(:has([data-status="under_review"])) tbody tr'

/** The first demand in the order book — the seeded maize demand, when nothing else was recorded earlier. */
const DEMAND_TO_OPEN = '[data-testid="demand-table"] tbody tr'

/** The link inside the wrapper the demand detail puts around the opportunity status. */
const OPPORTUNITY_TO_OPEN = '[data-testid="opportunity-link"] a'

export const OPS_PROGRAMME_CHAPTERS: Chapter[] = [
  {
    id: 'welcome',
    titleKey: 'tour.ops.chapter.welcome',
    steps: [
      {
        route: '/ops',
        testId: 'ops-home',
        titleKey: 'tour.ops.welcomeTitle',
        bodyKey: 'tour.ops.welcomeBody',
        placement: 'center',
      },
      {
        route: '/ops',
        testId: 'nav-sidebar',
        titleKey: 'tour.ops.welcomeSidebarTitle',
        bodyKey: 'tour.ops.welcomeSidebarBody',
      },
    ],
  },
  {
    id: 'queues',
    titleKey: 'tour.ops.chapter.queues',
    steps: [
      {
        route: '/ops',
        testId: 'ops-queue-requests',
        feature: 'resources',
        titleKey: 'tour.ops.queueTitle',
        bodyKey: 'tour.ops.queueBody',
      },
      {
        route: '/ops',
        testId: 'ops-queue-demands',
        titleKey: 'tour.ops.queuesDemandsTitle',
        bodyKey: 'tour.ops.queuesDemandsBody',
      },
      {
        route: '/ops',
        testId: 'ops-queue-verification',
        titleKey: 'tour.ops.queuesVerificationTitle',
        bodyKey: 'tour.ops.queuesVerificationBody',
      },
    ],
  },
  {
    id: 'requests',
    feature: 'resources',
    titleKey: 'tour.ops.chapter.requests',
    steps: [
      // The spotlight is the toolbar that holds both filters, so the person can
      // reach either. The select menus are portals that draw above the tour's
      // overlay; the bubble sits to the right so a menu never opens under it.
      {
        route: '/ops/requests',
        testId: 'requests-filters',
        titleKey: 'tour.ops.requestsFiltersTitle',
        bodyKey: 'tour.ops.requestsFiltersBody',
        placement: 'right',
        tryIt: true,
      },
      // Absent when the chosen filter matches nothing: then the screens behind
      // it are skipped too.
      {
        route: '/ops/requests',
        testId: 'requests-table',
        titleKey: 'tour.ops.requestsTitle',
        bodyKey: 'tour.ops.requestsBody',
        open: REQUEST_TO_REVIEW,
        optional: true,
      },
      {
        route: '/ops/requests/$requestId',
        testId: 'review-estimate',
        titleKey: 'tour.ops.requestsEstimateTitle',
        bodyKey: 'tour.ops.requestsEstimateBody',
      },
      // The three capacity figures depend on the village having a current
      // capacity row; the seeded villages do.
      {
        route: '/ops/requests/$requestId',
        testId: 'review-capacity-basis',
        titleKey: 'tour.ops.requestsCapacityTitle',
        bodyKey: 'tour.ops.requestsCapacityBody',
        optional: true,
      },
      {
        route: '/ops/requests/$requestId',
        testId: 'review-headroom',
        titleKey: 'tour.ops.requestsHeadroomTitle',
        bodyKey: 'tour.ops.requestsHeadroomBody',
        optional: true,
      },
      {
        route: '/ops/requests/$requestId',
        testId: 'review-peaks',
        titleKey: 'tour.ops.requestsPeaksTitle',
        bodyKey: 'tour.ops.requestsPeaksBody',
        optional: true,
      },
      // Explain-only, and only there for a request that awaits a decision.
      {
        route: '/ops/requests/$requestId',
        testId: 'review-decision',
        titleKey: 'tour.ops.requestsDecisionTitle',
        bodyKey: 'tour.ops.requestsDecisionBody',
        optional: true,
      },
    ],
  },
  {
    id: 'demand',
    titleKey: 'tour.ops.chapter.demand',
    steps: [
      {
        route: '/ops/demand',
        testId: 'demand-table',
        titleKey: 'tour.ops.demandTitle',
        bodyKey: 'tour.ops.demandBody',
      },
      // Explain-only; Next opens the first demand in the list.
      {
        route: '/ops/demand',
        testId: 'demand-create-open',
        titleKey: 'tour.ops.demandCreateTitle',
        bodyKey: 'tour.ops.demandCreateBody',
        open: DEMAND_TO_OPEN,
      },
      {
        route: '/ops/demand/$demandId',
        testId: 'demand-detail',
        titleKey: 'tour.ops.demandDetailTitle',
        bodyKey: 'tour.ops.demandDetailBody',
        placement: 'center',
      },
      // A demand with no overlapping supply has no rows to show — the coffee
      // demand's honest zero — so these depend on the demand that was opened.
      {
        route: '/ops/demand/$demandId',
        testId: 'demand-matches',
        titleKey: 'tour.ops.demandMatchesTitle',
        bodyKey: 'tour.ops.demandMatchesBody',
        optional: true,
      },
      {
        route: '/ops/demand/$demandId',
        testId: 'coverage-legend',
        titleKey: 'tour.ops.demandCoverageTitle',
        bodyKey: 'tour.ops.demandCoverageBody',
        optional: true,
      },
      // Present once a village has an opportunity on this demand (the seeded
      // maize demand has one). The "Create opportunity" button is not a stop of
      // its own: it only renders where no opportunity exists yet, so on the
      // seeded data the tour would wait for it in vain.
      {
        route: '/ops/demand/$demandId',
        testId: 'opportunity-link',
        titleKey: 'tour.ops.demandOpportunityLinkTitle',
        bodyKey: 'tour.ops.demandOpportunityLinkBody',
        open: OPPORTUNITY_TO_OPEN,
        optional: true,
      },
      {
        route: '/ops/opportunities/$opportunityId',
        testId: 'opportunity-detail',
        titleKey: 'tour.ops.demandOpportunityTitle',
        bodyKey: 'tour.ops.demandOpportunityBody',
        placement: 'center',
      },
      {
        route: '/ops/opportunities/$opportunityId',
        testId: 'opportunity-quantities',
        titleKey: 'tour.ops.demandQuantitiesTitle',
        bodyKey: 'tour.ops.demandQuantitiesBody',
      },
      // Gone once the opportunity is declined or lapsed.
      {
        route: '/ops/opportunities/$opportunityId',
        testId: 'opportunity-actions',
        titleKey: 'tour.ops.demandActionsTitle',
        bodyKey: 'tour.ops.demandActionsBody',
        optional: true,
      },
      {
        route: '/ops/opportunities/$opportunityId',
        testId: 'supply-lines',
        titleKey: 'tour.ops.demandSupplyLinesTitle',
        bodyKey: 'tour.ops.demandSupplyLinesBody',
        optional: true,
      },
      {
        route: '/ops/opportunities/$opportunityId',
        testId: 'opportunity-attach',
        titleKey: 'tour.ops.demandAttachTitle',
        bodyKey: 'tour.ops.demandAttachBody',
      },
    ],
  },
  {
    id: 'reference',
    titleKey: 'tour.ops.chapter.reference',
    steps: [
      {
        route: '/ops/catalogue',
        testId: 'catalogue-table',
        feature: 'resources',
        titleKey: 'tour.ops.referenceCatalogueTitle',
        bodyKey: 'tour.ops.referenceCatalogueBody',
      },
      {
        route: '/ops/catalogue',
        testId: 'catalogue-kinds',
        feature: 'resources',
        titleKey: 'tour.ops.referenceLoansTitle',
        bodyKey: 'tour.ops.referenceLoansBody',
      },
      // Only the button is lit, as with buyers: the form opens below, and
      // nothing is kept until it is submitted.
      {
        route: '/ops/catalogue',
        testId: 'catalogue-create-open',
        feature: 'resources',
        titleKey: 'tour.ops.referenceCatalogueCreateTitle',
        bodyKey: 'tour.ops.referenceCatalogueCreateBody',
        tryIt: true,
      },
      {
        route: '/ops/buyers',
        testId: 'buyers-table',
        titleKey: 'tour.ops.referenceBuyersTitle',
        bodyKey: 'tour.ops.referenceBuyersBody',
      },
      // Only the button is lit: the form it opens appears below the table,
      // outside the spotlight. Nothing is kept until the form is submitted, and
      // the tour leaves before that could happen.
      {
        route: '/ops/buyers',
        testId: 'buyer-create-open',
        titleKey: 'tour.ops.referenceBuyerCreateTitle',
        bodyKey: 'tour.ops.referenceBuyerCreateBody',
        tryIt: true,
      },
      {
        route: '/ops/villages',
        testId: 'villages-table',
        titleKey: 'tour.ops.referenceVillagesTitle',
        bodyKey: 'tour.ops.referenceVillagesBody',
      },
      {
        route: '/ops/villages',
        testId: 'village-basis',
        titleKey: 'tour.ops.referenceBasisTitle',
        bodyKey: 'tour.ops.referenceBasisBody',
      },
    ],
  },
]
