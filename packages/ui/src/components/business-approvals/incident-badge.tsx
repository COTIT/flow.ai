import { Badge } from "../badge"
import { label } from "./labels"
import type { IncidentKind, Translate } from "./types"

/** An incident on a request: what kind, and whether it is still open. Words carry the state. */
export function IncidentBadge({
  kind,
  status,
  t,
}: {
  kind: IncidentKind
  status: "open" | "resolved"
  t: Translate
}) {
  return (
    <Badge variant={status === "open" ? "red" : "slate"}>
      {label(t, "incident", kind)} ·{" "}
      {t(status === "open" ? "card.incidentOpen" : "card.incidentResolved")}
    </Badge>
  )
}
