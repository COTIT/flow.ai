/** AFM's market dates (from their events page). Names and places are proper nouns: not translated. */
export const EVENTS = [
  {
    name: "Mile High Holiday Mart",
    place: "University of Denver, CO",
    start: "2026-11-06",
    end: "2026-11-08",
  },
  {
    name: "Second Star Farmers Market",
    place: "Denver, CO",
    start: "2026-11-08",
    end: "2026-11-08",
  },
  {
    name: "Cherry Creek Tree Lighting Bazaar",
    place: "Denver, CO",
    start: "2026-11-14",
    end: "2026-11-14",
  },
  {
    name: "Sloan's Lake Holiday Bazaar",
    place: "Denver, CO",
    start: "2026-12-05",
    end: "2026-12-05",
  },
  {
    name: "Fort Collins Holiday Gift Festival",
    place: "Fort Collins, CO",
    start: "2026-12-06",
    end: "2026-12-06",
  },
  {
    name: "Last Chance Gift Fest",
    place: "Boulder County Fairgrounds, CO",
    start: "2026-12-19",
    end: "2026-12-20",
  },
] as const

/** Markets not yet over on `today` (YYYY-MM-DD), soonest first. */
export const upcoming = (today: string) => EVENTS.filter((e) => e.end >= today)
