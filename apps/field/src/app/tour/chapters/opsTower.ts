import type { Chapter } from '@/app/tour/tourTypes'

/**
 * The ops tour, last part: the Control Tower (a chapter of the ops surface, not a
 * role of its own) and the closing hand-off.
 *
 * Every stop is explain-only unless it says `tryIt`; the tour opens rows but
 * never presses a button that writes. Approve, Reject, Publish and Void cannot
 * be undone, so they are explained here and never performed.
 *
 * The Tower is always about one village, so everything after the picker is
 * behind a gate: the person picks the village themselves (there is no auto-pick)
 * and the tiles appear. The figures inside the energy tile can be absent for a
 * village without a capacity row, so those stops are optional; the tile
 * wrappers themselves always render once a village is chosen.
 */
export const OPS_TOWER_CHAPTERS: Chapter[] = [
  {
    id: 'tower',
    titleKey: 'tour.ops.chapter.tower',
    steps: [
      {
        route: '/ops/tower',
        testId: 'tower',
        titleKey: 'tour.ops.towerTitle',
        bodyKey: 'tour.ops.towerBody',
        placement: 'center',
      },
      {
        // A base-ui Select: its list opens in a portal at z-40, after the
        // tour's overlay in the document, so the options stay clickable.
        route: '/ops/tower',
        testId: 'tower-village',
        titleKey: 'tour.ops.towerVillageTitle',
        bodyKey: 'tour.ops.towerVillageBody',
        tryIt: true,
        gate: 'tile-production',
      },
      {
        route: '/ops/tower',
        testId: 'tower-capacity',
        titleKey: 'tour.ops.towerCapacityTitle',
        bodyKey: 'tour.ops.towerCapacityBody',
        optional: true,
      },
      {
        route: '/ops/tower',
        testId: 'tower-prospective-peak',
        titleKey: 'tour.ops.towerProspectiveTitle',
        bodyKey: 'tour.ops.towerProspectiveBody',
        optional: true,
      },
      {
        route: '/ops/tower',
        testId: 'tower-approved-peak',
        titleKey: 'tour.ops.towerApprovedTitle',
        bodyKey: 'tour.ops.towerApprovedBody',
        optional: true,
      },
      {
        route: '/ops/tower',
        testId: 'tower-headroom',
        titleKey: 'tour.ops.towerHeadroomTitle',
        bodyKey: 'tour.ops.towerHeadroomBody',
        optional: true,
      },
      {
        route: '/ops/tower',
        testId: 'tile-market',
        titleKey: 'tour.ops.towerMarketTitle',
        bodyKey: 'tour.ops.towerMarketBody',
      },
      {
        route: '/ops/tower',
        testId: 'tile-pue',
        feature: 'resources',
        titleKey: 'tour.ops.towerPueTitle',
        bodyKey: 'tour.ops.towerPueBody',
      },
      {
        route: '/ops/tower',
        testId: 'tile-quality',
        titleKey: 'tour.ops.towerQualityTitle',
        bodyKey: 'tour.ops.towerQualityBody',
      },
      {
        // Last on this screen, because opening its records has to be. The
        // selector asks for the link itself (`a`): while the tile is still
        // loading the same test id sits on plain text, and pressing that would
        // open nothing.
        route: '/ops/tower',
        testId: 'tile-production',
        titleKey: 'tour.ops.towerProductionTitle',
        bodyKey: 'tour.ops.towerProductionBody',
        open: '[data-testid="tile-production"] a[data-testid="tile-drill"]',
      },
      {
        // A plain route reached by a click, because it needs the village the
        // click carries.
        route: '/ops/tower/production',
        testId: 'production-table',
        titleKey: 'tour.ops.towerProductionTableTitle',
        bodyKey: 'tour.ops.towerProductionTableBody',
        arrive: true,
        optional: true,
      },
      {
        // The whole first row, not one link: the crop and the farmer's name
        // are two drill links in it.
        route: '/ops/tower/production',
        testId: 'production-row',
        titleKey: 'tour.ops.towerDrillTitle',
        bodyKey: 'tour.ops.towerDrillBody',
        arrive: true,
        optional: true,
      },
    ],
  },
  {
    id: 'next',
    titleKey: 'tour.ops.chapter.next',
    demoOnly: true,
    steps: [
      {
        route: '/ops',
        testId: 'ops-home',
        titleKey: 'tour.ops.nextTitle',
        bodyKey: 'tour.ops.nextBody',
        placement: 'center',
      },
    ],
  },
]
