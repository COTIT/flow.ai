import { Preview } from "./preview"

const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

/**
 * The document inside /storefront's preview frame (the only route whose CSP allows framing,
 * by Flow itself). Outside the (app) group: no shell, no assistant. Only a revision id comes
 * in by the URL; anything else is ignored.
 */
export default async function StorefrontPreviewPage({
  searchParams,
}: {
  searchParams: Promise<{ revision?: string | string[] }>
}) {
  const { revision } = await searchParams
  return (
    <Preview
      revisionId={
        typeof revision === "string" && uuid.test(revision) ? revision : null
      }
    />
  )
}
