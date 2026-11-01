/**
 * Every seeded user has already taken every tour, as far as the suite is
 * concerned.
 *
 * The guided tour runs itself once per user per surface on a first visit, and
 * a Playwright context is always a first visit: without this, a full-screen
 * overlay would open on top of all 173 tests and every click would land on the
 * scrim instead of the app.
 *
 * Written as real storage rather than a test-only escape hatch in the app: the
 * app has no "suppress tours" flag to reach for, and giving it one would put a
 * branch in shipped code that exists only for the suite. `tour.spec.ts` opts
 * out of this state to test the tour itself.
 */
const SEEDED_USERS = [
  '80000000-0000-4000-8000-000000000001', // admin
  '80000000-0000-4000-8000-000000000002', // ops
  '80000000-0000-4000-8000-000000000003', // officer, Ilundo
  '80000000-0000-4000-8000-000000000004', // officer, Mgama
  '80000000-0000-4000-8000-000000000005', // farmer, Neema
  '80000000-0000-4000-8000-000000000006', // farmer, Joseph
  '80000000-0000-4000-8000-000000000007', // officer, Ilundo (second; seed_surveys.sql)
]

const SURFACES = ['farmer', 'officer', 'ops']

export const TOURS_ALREADY_SEEN = {
  cookies: [],
  origins: [
    {
      origin: 'http://localhost:5173',
      localStorage: [
        {
          name: 'ruaha360:tours-seen',
          value: JSON.stringify(
            SEEDED_USERS.flatMap((user) => SURFACES.map((surface) => `${surface}:${user}`)),
          ),
        },
      ],
    },
  ],
}
