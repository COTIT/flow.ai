import { Button } from "@genesy/ui/components/button"

/** Prev/next paging from an API list's `meta`; texts come from the app. */
function Pager({
  page,
  lastPage,
  summary,
  previous,
  next,
  onPage,
}: {
  page: number
  lastPage: number
  summary: string
  previous: string
  next: string
  onPage: (page: number) => void
}) {
  return (
    <div className="flex items-center gap-2 text-sm text-muted-foreground">
      <span className="mr-auto tabular-nums">{summary}</span>
      <Button
        variant="outline"
        size="sm"
        disabled={page <= 1}
        onClick={() => onPage(page - 1)}
      >
        {previous}
      </Button>
      <Button
        variant="outline"
        size="sm"
        disabled={page >= lastPage}
        onClick={() => onPage(page + 1)}
      >
        {next}
      </Button>
    </div>
  )
}

export { Pager }
