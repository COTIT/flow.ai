import type { Chapter } from '@/app/tour/tourTypes'

/**
 * The officer tour, one chapter per module, in the order the work happens.
 *
 * Every stop is explain-only unless it says `tryIt`; the tour opens rows but
 * never presses a button that writes. Register, Verify, Redeem and resetting a
 * farmer's login cannot be undone, so they are explained here and never
 * performed — the presenter does them live on a record they made themselves.
 *
 * Stops on a tall anchor (a whole screen, a long list) sit `center`, so the
 * bubble is never parked off the bottom of a phone.
 */
export const OFFICER_CHAPTERS: Chapter[] = [
  {
    id: 'welcome',
    titleKey: 'tour.officer.chapter.welcome',
    steps: [
      {
        route: '/officer',
        testId: 'officer-home',
        titleKey: 'tour.officer.welcomeTitle',
        bodyKey: 'tour.officer.welcomeBody',
        placement: 'center',
      },
      {
        route: '/officer',
        testId: 'nav-tabs',
        titleKey: 'tour.officer.welcomeTabsTitle',
        bodyKey: 'tour.officer.welcomeTabsBody',
      },
    ],
  },
  {
    id: 'home',
    titleKey: 'tour.officer.chapter.home',
    steps: [
      {
        // Rendered once the village counts have loaded, for an officer with an
        // assigned village (every seeded officer has one). Not optional: an
        // optional stop waits only 4 s, and the counts are several queries.
        route: '/officer',
        testId: 'officer-home-register',
        titleKey: 'tour.officer.homeRegisterTitle',
        bodyKey: 'tour.officer.homeRegisterBody',
      },
      {
        // The old body says "this counts what is waiting for you" and stops
        // there; the count also holds households only a second person can clear.
        route: '/officer',
        testId: 'officer-unverified',
        titleKey: 'tour.officer.outstandingTitle',
        bodyKey: 'tour.officer.homeUnverifiedBody',
      },
      {
        route: '/officer',
        testId: 'officer-village',
        titleKey: 'tour.officer.homeVillageTitle',
        bodyKey: 'tour.officer.homeVillageBody',
        optional: true,
      },
    ],
  },
  {
    id: 'register',
    titleKey: 'tour.officer.chapter.register',
    steps: [
      {
        route: '/officer/register',
        testId: 'register-progress',
        titleKey: 'tour.officer.registerTitle',
        bodyKey: 'tour.officer.registerBody',
      },
      {
        // The whole group, so the spotlight holds every field the person is
        // asked to type in (names and phone). Typing only writes a local draft.
        route: '/officer/register',
        testId: 'register-group-person',
        titleKey: 'tour.officer.registerDraftTitle',
        bodyKey: 'tour.officer.registerDraftBody',
        tryIt: true,
      },
      {
        route: '/officer/register',
        testId: 'register-phone',
        titleKey: 'tour.officer.registerPhoneTitle',
        bodyKey: 'tour.officer.registerPhoneBody',
      },
      {
        // The group rather than `register-gps-status`, which only renders while
        // the phone is looking for a position or has failed to find one.
        route: '/officer/register',
        testId: 'register-group-farm',
        titleKey: 'tour.officer.registerFarmTitle',
        bodyKey: 'tour.officer.registerFarmBody',
      },
      {
        route: '/officer/register',
        testId: 'register-confidence',
        titleKey: 'tour.officer.registerConfidenceTitle',
        bodyKey: 'tour.officer.registerConfidenceBody',
      },
      {
        route: '/officer/register',
        testId: 'register-submit',
        titleKey: 'tour.officer.registerSubmitTitle',
        bodyKey: 'tour.officer.registerSubmitBody',
      },
    ],
  },
  {
    id: 'people',
    titleKey: 'tour.officer.chapter.people',
    steps: [
      {
        // The toolbar holds both the search box and the verification filter, so
        // the spotlight contains everything the person is asked to use. The
        // filter's menu is a portal (z-40, after the tour's overlay).
        route: '/officer/people',
        testId: 'people-toolbar',
        titleKey: 'tour.officer.peopleFindTitle',
        bodyKey: 'tour.officer.peopleFindBody',
        tryIt: true,
      },
      {
        // Next presses the first row, which is a `<tr onClick>` that navigates.
        // Named through the table because a row's own test id is passed as a
        // prop (`rowTestId`), which the anchors test cannot see. A search that
        // left no rows means no table, so the person's screens are skipped.
        route: '/officer/people',
        testId: 'people-table',
        titleKey: 'tour.officer.peopleListTitle',
        bodyKey: 'tour.officer.peopleRowsBody',
        placement: 'center',
        open: '[data-testid="people-table"] tbody tr',
        optional: true,
      },
      {
        route: '/officer/people/$personId',
        testId: 'person-detail',
        titleKey: 'tour.officer.peopleDetailTitle',
        bodyKey: 'tour.officer.peoplePersonBody',
        placement: 'center',
      },
      {
        route: '/officer/people/$personId',
        testId: 'edit-person',
        titleKey: 'tour.officer.peopleEditTitle',
        bodyKey: 'tour.officer.peopleEditBody',
        optional: true,
      },
      {
        route: '/officer/people/$personId',
        testId: 'person-outstanding',
        titleKey: 'tour.officer.peopleOutstandingTitle',
        bodyKey: 'tour.officer.peopleOutstandingBody',
        optional: true,
      },
      {
        route: '/officer/people/$personId',
        testId: 'app-login',
        titleKey: 'tour.officer.peopleLoginTitle',
        bodyKey: 'tour.officer.peopleLoginBody',
      },
    ],
  },
  {
    id: 'verify',
    titleKey: 'tour.officer.chapter.verify',
    steps: [
      {
        route: '/officer/verify',
        testId: 'verify-queue',
        titleKey: 'tour.officer.verifyTitle',
        bodyKey: 'tour.officer.verifyBody',
        placement: 'center',
      },
      {
        route: '/officer/verify',
        testId: 'verify-queue-count',
        titleKey: 'tour.officer.verifyCountTitle',
        bodyKey: 'tour.officer.verifyCountBody',
        optional: true,
      },
      {
        // Explain-only: the row holds a Verify button, which is one-way.
        route: '/officer/verify',
        testId: 'verify-queue-row',
        titleKey: 'tour.officer.verifyRowTitle',
        bodyKey: 'tour.officer.verifyRowCheckBody',
        optional: true,
      },
    ],
  },
  {
    id: 'redeem',
    titleKey: 'tour.officer.chapter.redeem',
    steps: [
      {
        route: '/officer/redeem',
        testId: 'redeem-screen',
        titleKey: 'tour.officer.redeemTitle',
        bodyKey: 'tour.officer.redeemBody',
        placement: 'center',
      },
      {
        route: '/officer/redeem',
        testId: 'redeem-scan',
        titleKey: 'tour.officer.redeemScanTitle',
        bodyKey: 'tour.officer.redeemScanBody',
      },
      {
        route: '/officer/redeem',
        testId: 'redeem-code',
        titleKey: 'tour.officer.redeemCodeTitle',
        bodyKey: 'tour.officer.redeemCodeBody',
      },
      {
        // Explain-only: every look-up writes a 'scanned' event to the trail.
        // What follows a look-up (the preview, the ID check) cannot be anchored,
        // so it lives in the copy.
        route: '/officer/redeem',
        testId: 'redeem-lookup',
        titleKey: 'tour.officer.redeemLookupTitle',
        bodyKey: 'tour.officer.redeemLookupBody',
      },
    ],
  },
  {
    id: 'next',
    titleKey: 'tour.officer.chapter.next',
    demoOnly: true,
    steps: [
      {
        route: '/officer',
        testId: 'officer-home',
        titleKey: 'tour.officer.nextTitle',
        bodyKey: 'tour.officer.nextBody',
        placement: 'center',
      },
    ],
  },
]
