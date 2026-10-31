The acceptance journey from CLAUDE.md lives in `journey.spec.ts`: eight steps,
four roles, one continuous test, run against the demo seed. Every other spec
covers a single screen and its loading / empty / error / edge states.

Every record the suite creates carries the `E2E-` marker, and cleanup removes
only marked rows — see `support/marker.ts` and `support/cleanup.sql`. Nothing
seeded is ever verified or decided: `app_verify` has no inverse and approval is
terminal, so a decision on a seeded record cannot be undone.

    pnpm e2e                 # everything
    pnpm e2e journey         # the acceptance journey alone
