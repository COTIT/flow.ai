import { Badge } from "@genesy/ui/components/badge"

// Colour by status word only; an unknown status still shows, in the neutral style.
const tones: Record<
  string,
  "default" | "secondary" | "destructive" | "outline"
> = {
  released: "default",
  active: "default",
  quarantine: "secondary",
  recalled: "destructive",
  revoked: "destructive",
}

/** The API's status word as a badge; `label` is its translation. */
export function StatusBadge({
  status,
  label,
}: {
  status: string
  label?: string
}) {
  return <Badge variant={tones[status] ?? "outline"}>{label ?? status}</Badge>
}
