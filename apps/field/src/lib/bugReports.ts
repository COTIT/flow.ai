import type { TrailEntry } from '@/lib/bugTrail'
import { redactUrl } from '@/lib/bugTrail'
import type { Json } from '@/lib/db.types'
import { newUuid } from '@/lib/ids'
import { supabase } from '@/lib/supabase'

/**
 * Bug reports from the floating button (migration 20261025090001_bug_reports).
 *
 * Anyone signed in files; only admin reads, so the insert asks for nothing
 * back. The bucket and the table decide what is accepted — a refusal comes
 * back with their own message.
 */
export const BUG_REPORT_BUCKET = 'bug-reports'

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
  reporterId: string
  description: string
  actions: TrailEntry[]
  screenshot: Blob | null
  photo: File | null
}

const EXTENSION: Record<string, string> = { 'image/png': 'png', 'image/jpeg': 'jpg', 'image/webp': 'webp', 'image/heic': 'heic' }

/** `{reporter}/{uuid}.{ext}` — the storage policy reads the owner from the first folder. */
async function store(reporterId: string, image: Blob | null): Promise<string | null> {
  if (!image) return null
  const path = `${reporterId}/${newUuid()}.${EXTENSION[image.type] ?? 'png'}`
  const { error } = await supabase.storage
    .from(BUG_REPORT_BUCKET)
    .upload(path, image, { contentType: image.type, upsert: false })
  if (error) throw new Error(error.message)
  return path
}

export async function submitBugReport(input: BugReportInput): Promise<void> {
  const screenshot_path = await store(input.reporterId, input.screenshot)
  const photo_path = await store(input.reporterId, input.photo)
  const { error } = await supabase.from('bug_report').insert({
    route: redactUrl(location.href),
    user_agent: navigator.userAgent,
    viewport: `${window.innerWidth}x${window.innerHeight}`,
    description: input.description,
    actions: input.actions as unknown as Json,
    screenshot_path,
    photo_path,
  })
  if (error) throw new Error(error.message)
}
