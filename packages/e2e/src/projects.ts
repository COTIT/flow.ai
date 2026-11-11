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

/** Flow's tablet check: an 11" iPad in portrait, touch. */
export const tablet = {
  ...devices["Desktop Chrome"],
  viewport: { width: 834, height: 1112 },
  hasTouch: true,
}

/** iPad sizes for the till (it runs on an iPad at the counter): landscape and portrait, touch. */
export const tablets = {
  "ipad-landscape": {
    ...devices["Desktop Chrome"],
    viewport: { width: 1024, height: 768 },
    hasTouch: true,
  },
  "ipad-portrait": {
    ...devices["Desktop Chrome"],
    viewport: { width: 834, height: 1194 },
    hasTouch: true,
  },
}

/**
 * Desktop and 375 px projects for one app, e.g. `...projectsFor("office", { testDir, baseURL })`.
 * `tablets: true` adds the two iPad projects; `tablet: true` adds one 834 px portrait project.
 */
export function projectsFor(
  app: string,
  {
    testDir,
    baseURL,
    tablets: withTablets = false,
    tablet: withTablet = false,
  }: { testDir: string; baseURL: string; tablets?: boolean; tablet?: boolean }
): Project[] {
  const sizes = {
    ...viewports,
    ...(withTablets ? tablets : {}),
    ...(withTablet ? { tablet } : {}),
  }
  return Object.entries(sizes).map(([name, use]) => ({
    name: `${app}-${name}`,
    testDir,
    use: { ...use, baseURL },
  }))
}

/** The project without service workers: one would route page requests past page.route mocks. */
export function blockServiceWorkers(p: Project): Project {
  return { ...p, use: { ...p.use, serviceWorkers: "block" } }
}
