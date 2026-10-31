import { devices, type Project } from "@playwright/test"

/** The two viewports every screen is checked at. Chromium only. */
export const viewports = {
  desktop: {
    ...devices["Desktop Chrome"],
    viewport: { width: 1280, height: 800 },
  },
  mobile: {
    ...devices["Desktop Chrome"],
    viewport: { width: 375, height: 812 },
    isMobile: true,
    hasTouch: true,
  },
}

/** Desktop and 375 px projects for one app, e.g. `...projectsFor("office", { testDir, baseURL })`. */
export function projectsFor(
  app: string,
  { testDir, baseURL }: { testDir: string; baseURL: string }
): Project[] {
  return Object.entries(viewports).map(([name, use]) => ({
    name: `${app}-${name}`,
    testDir,
    use: { ...use, baseURL },
  }))
}
