import { Badge } from "@genesy/ui/components/badge"

type Tone = "teal" | "gold" | "red" | "slate"

// Colour by status word only (glass.css badge tones); an unknown status still shows, in slate.
const tones: Record<string, Tone> = {
  active: "teal",
  released: "teal",
  paid: "teal",
  completed: "teal",
  accepted: "teal",
  received: "teal",
  quarantine: "gold",
  awaiting_payment: "gold",
  pending: "gold",
  draft: "gold",
  partially_returned: "gold",
  partially_paid: "gold",
  recalled: "red",
  revoked: "red",
  denied: "red",
  cancelled: "red",
  disabled: "slate",
  nla: "slate",
}

/** The API's status word as a badge; `label` is its translation. */
export function StatusBadge({
  status,
  label,
  tone,
}: {
  status: string
  label?: string
  /** Overrides the colour picked from the status word. */
  tone?: Tone
}) {
  return <Badge variant={tone ?? tones[status] ?? "slate"}>{label ?? status}</Badge>
}
