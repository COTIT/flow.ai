import type { Chapter } from '@/app/tour/tourTypes'

/**
 * The farmer tour, one chapter per module, in the order the tabs run and then
 * the voucher the last tab leads to.
 *
 * Every stop is explain-only unless it says `tryIt`; the tour opens rows but
 * never presses a button that writes. Submitting a request and submitting a
 * survey cannot be undone (the survey answers once and issues a voucher), so
 * both are explained here and never performed.
 *
 * A stop that depends on rows the seeded farmer may not have is `optional`,
 * and so is anything after the first stop on a screen that could be absent.
 * The first stop on a screen that always renders is not: it carries the wait
 * for the screen's data, and that wait is 10 s rather than 4.
 */
export const FARMER_CHAPTERS: Chapter[] = [
  {
    id: 'welcome',
    titleKey: 'tour.farmer.chapter.welcome',
    steps: [
      {
        route: '/farm',
        testId: 'farm-home',
        titleKey: 'tour.farmer.welcomeTitle',
        bodyKey: 'tour.farmer.welcomeBody',
        placement: 'center',
      },
      {
        route: '/farm',
        testId: 'nav-tabs',
        titleKey: 'tour.farmer.welcomeTabsTitle',
        bodyKey: 'tour.farmer.welcomeTabsBody',
        placement: 'top',
      },
    ],
  },
  {
    id: 'home',
    titleKey: 'tour.farmer.chapter.home',
    steps: [
      {
        route: '/farm',
        testId: 'farm-home-summary',
        titleKey: 'tour.farmer.summaryTitle',
        bodyKey: 'tour.farmer.summaryBody',
      },
      {
        route: '/farm',
        testId: 'farm-home-surveys',
        titleKey: 'tour.farmer.homeSurveysTitle',
        bodyKey: 'tour.farmer.homeSurveysBody',
        optional: true,
      },
      {
        route: '/farm',
        testId: 'farm-home-opportunities',
        titleKey: 'tour.farmer.homeOpportunitiesTitle',
        bodyKey: 'tour.farmer.homeOpportunitiesBody',
        optional: true,
      },
      {
        route: '/farm',
        testId: 'farm-home-latest-request',
        feature: 'resources',
        titleKey: 'tour.farmer.homeLatestRequestTitle',
        bodyKey: 'tour.farmer.homeLatestRequestBody',
        optional: true,
      },
    ],
  },
  {
    id: 'myFarm',
    titleKey: 'tour.farmer.chapter.myFarm',
    steps: [
      {
        route: '/farm/my-farm',
        testId: 'my-farm',
        titleKey: 'tour.farmer.recordsTitle',
        bodyKey: 'tour.farmer.recordsBody',
      },
      {
        // The first badge on the page is the farm's own, near the top.
        route: '/farm/my-farm',
        testId: 'provenance-badge',
        titleKey: 'tour.farmer.myFarmBadgeTitle',
        bodyKey: 'tour.farmer.myFarmBadgeBody',
        optional: true,
      },
      {
        route: '/farm/my-farm',
        testId: 'plot-card',
        titleKey: 'tour.farmer.myFarmPlotTitle',
        bodyKey: 'tour.farmer.myFarmPlotBody',
        optional: true,
      },
      {
        route: '/farm/my-farm',
        testId: 'cycle-card',
        titleKey: 'tour.farmer.myFarmCropTitle',
        bodyKey: 'tour.farmer.myFarmCropBody',
        optional: true,
      },
    ],
  },
  {
    id: 'equipment',
    feature: 'resources',
    titleKey: 'tour.farmer.chapter.equipment',
    steps: [
      {
        route: '/farm/equipment',
        testId: 'equipment-list',
        titleKey: 'tour.farmer.equipmentListTitle',
        bodyKey: 'tour.farmer.equipmentListBody',
        // The whole list is taller than a phone screen, so a bubble placed beside it
        // lands off the edge; centred, it is always readable.
        placement: 'center',
      },
      {
        // The <li> around the card's link, so the whole card is lit and the
        // click lands on the link itself. Navigation only.
        route: '/farm/equipment',
        testId: 'equipment-item',
        titleKey: 'tour.farmer.equipmentOpenTitle',
        bodyKey: 'tour.farmer.equipmentOpenBody',
        open: '[data-testid="equipment-item"] a',
        optional: true,
      },
      {
        // Rated power and the indicative price, side by side.
        route: '/farm/equipment/$equipmentId',
        testId: 'equipment-specs',
        titleKey: 'tour.farmer.equipmentPriceTitle',
        bodyKey: 'tour.farmer.equipmentPriceBody',
      },
      {
        // Explained, never pressed: the button sends a request to ops.
        route: '/farm/equipment/$equipmentId',
        testId: 'request-submit',
        titleKey: 'tour.farmer.equipmentSubmitTitle',
        bodyKey: 'tour.farmer.equipmentSubmitBody',
      },
    ],
  },
  {
    id: 'requests',
    feature: 'resources',
    titleKey: 'tour.farmer.chapter.requests',
    steps: [
      {
        // Opens an APPROVED request rather than the first row: the seeded
        // requests share one created_at, so "first" is not a stable choice,
        // and the approved one shows every part of the detail screen.
        route: '/farm/requests',
        testId: 'requests-list',
        titleKey: 'tour.farmer.requestsTitle',
        bodyKey: 'tour.farmer.requestsBody',
        open: '[data-testid="my-request-row"][data-status="approved"]',
        optional: true,
      },
      {
        route: '/farm/requests/$requestId',
        testId: 'request-assumptions',
        titleKey: 'tour.farmer.requestsAssumptionsTitle',
        bodyKey: 'tour.farmer.requestsAssumptionsBody',
      },
      {
        route: '/farm/requests/$requestId',
        testId: 'request-stored-estimate',
        titleKey: 'tour.farmer.requestsEstimateTitle',
        bodyKey: 'tour.farmer.requestsEstimateBody',
      },
      {
        route: '/farm/requests/$requestId',
        testId: 'request-decision',
        titleKey: 'tour.farmer.requestsDecisionTitle',
        bodyKey: 'tour.farmer.requestsDecisionBody',
        optional: true,
      },
    ],
  },
  {
    id: 'opportunities',
    titleKey: 'tour.farmer.chapter.opportunities',
    steps: [
      {
        route: '/farm/opportunities',
        testId: 'farmer-opportunities',
        titleKey: 'tour.farmer.opportunitiesTitle',
        bodyKey: 'tour.farmer.opportunitiesBody',
      },
      {
        route: '/farm/opportunities',
        testId: 'my-contribution',
        titleKey: 'tour.farmer.opportunitiesShareTitle',
        bodyKey: 'tour.farmer.opportunitiesShareBody',
        optional: true,
      },
      {
        route: '/farm/opportunities',
        testId: 'offered-total',
        titleKey: 'tour.farmer.opportunitiesTotalTitle',
        bodyKey: 'tour.farmer.opportunitiesTotalBody',
        optional: true,
      },
    ],
  },
  {
    id: 'surveys',
    titleKey: 'tour.farmer.chapter.surveys',
    steps: [
      {
        route: '/farm/surveys',
        testId: 'surveys-list',
        titleKey: 'tour.farmer.surveysTitle',
        bodyKey: 'tour.farmer.surveysBody',
      },
      {
        route: '/farm/surveys',
        testId: 'survey-card',
        titleKey: 'tour.farmer.surveysCardTitle',
        bodyKey: 'tour.farmer.surveysCardBody',
        optional: true,
      },
      {
        route: '/farm/surveys',
        testId: 'survey-incentive',
        titleKey: 'tour.farmer.surveysIncentiveTitle',
        bodyKey: 'tour.farmer.surveysIncentiveBody',
        optional: true,
      },
      {
        // A link that only opens the questions. Nothing is sent by opening.
        route: '/farm/surveys',
        testId: 'survey-open',
        titleKey: 'tour.farmer.surveysOpenTitle',
        bodyKey: 'tour.farmer.surveysOpenBody',
        open: '[data-testid="survey-open"]',
        optional: true,
      },
      {
        route: '/farm/surveys/$surveyId',
        testId: 'survey-question',
        titleKey: 'tour.farmer.surveysQuestionsTitle',
        bodyKey: 'tour.farmer.surveysQuestionsBody',
      },
      {
        // Explained, never pressed: answering is once per household and issues
        // the voucher.
        route: '/farm/surveys/$surveyId',
        testId: 'survey-submit',
        titleKey: 'tour.farmer.surveysSubmitTitle',
        bodyKey: 'tour.farmer.surveysSubmitBody',
        optional: true,
      },
    ],
  },
  {
    id: 'voucher',
    titleKey: 'tour.farmer.chapter.voucher',
    steps: [
      {
        // The first answered survey. For Neema that is the energy one, whose
        // voucher is still waiting to be collected.
        route: '/farm/surveys',
        testId: 'survey-view-voucher',
        titleKey: 'tour.farmer.voucherViewTitle',
        bodyKey: 'tour.farmer.voucherViewBody',
        open: '[data-testid="survey-view-voucher"]',
        optional: true,
      },
      {
        route: '/farm/surveys/$surveyId',
        testId: 'voucher-card',
        titleKey: 'tour.farmer.voucherSlipTitle',
        bodyKey: 'tour.farmer.voucherSlipBody',
      },
      {
        route: '/farm/surveys/$surveyId',
        testId: 'voucher-code',
        titleKey: 'tour.farmer.voucherCodeTitle',
        bodyKey: 'tour.farmer.voucherCodeBody',
        optional: true,
      },
      {
        route: '/farm/surveys/$surveyId',
        testId: 'voucher-amount',
        titleKey: 'tour.farmer.voucherAmountTitle',
        bodyKey: 'tour.farmer.voucherAmountBody',
        optional: true,
      },
      {
        route: '/farm/surveys/$surveyId',
        testId: 'voucher-validity',
        titleKey: 'tour.farmer.voucherValidityTitle',
        bodyKey: 'tour.farmer.voucherValidityBody',
        optional: true,
      },
      {
        route: '/farm/surveys/$surveyId',
        testId: 'audit-timeline',
        titleKey: 'tour.farmer.voucherTrailTitle',
        bodyKey: 'tour.farmer.voucherTrailBody',
        optional: true,
      },
    ],
  },
  {
    id: 'next',
    titleKey: 'tour.farmer.chapter.next',
    steps: [
      {
        route: '/farm',
        testId: 'farm-home',
        titleKey: 'tour.farmer.nextFarmTitle',
        bodyKey: 'tour.farmer.nextFarmBody',
        placement: 'center',
        demoOnly: true,
      },
    ],
  },
]
