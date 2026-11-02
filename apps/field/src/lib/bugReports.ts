import { farmBugReportsCreate } from '@genesy/api-client/endpoints/farm-bug-reports'

import { workspaceId } from '@/app/workspace'
import type { TrailEntry } from '@/lib/bugTrail'
import { redactUrl } from '@/lib/bugTrail'

/**
 * Bug reports from the floating button: `POST farm/bug-reports` (multipart).
 *
 * Anyone with a farm role files, always under their own name (the API takes the reporter from the
 * session; naming one is an unknown field). Only admin reads. The API decides what is accepted —
 * image type and size, at least one image, at most 30 steps — and a refusal shows its message.
 */
const SCREENSHOT_TIMEOUT_MS = 8000

/**
 * A PNG of the page as it is now, without the bug reporter itself. Null when
 * the browser cannot draw it in time — the sheet then asks for a photo.
 */
export async function captureScreen(): Promise<Blob | null> {
  try {
    const { toBlob } = await import('html-to-image')
    const shot = toBlob(document.body, {
      pixelRatio: 1,
      filter: (node) => !(node instanceof HTMLElement && node.hasAttribute('data-bug-report')),
    })
    const timeout = new Promise<null>((resolve) => setTimeout(() => resolve(null), SCREENSHOT_TIMEOUT_MS))
    return await Promise.race([shot, timeout])
  } catch {
    return null
  }
}

export interface BugReportInput {
  /** Made when the sheet opens and kept until the report is filed, so a retry is filed once. */
  id: string
  description: string
  actions: TrailEntry[]
  screenshot: Blob | null
  photo: File | null
}

export async function submitBugReport(input: BugReportInput): Promise<void> {
  await farmBugReportsCreate(workspaceId(), {
    id: input.id,
    route: redactUrl(location.href),
    user_agent: navigator.userAgent,
    viewport: `${window.innerWidth}x${window.innerHeight}`,
    description: input.description,
    actions: JSON.stringify(input.actions),
    screenshot: input.screenshot,
    photo: input.photo,
  })
}
