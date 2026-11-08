/** Contextual update so the agent states times in the workspace zone, not the device's or its own guess. */
export function timeZoneUpdate(zone = "UTC", now = new Date()) {
  const offset =
    new Intl.DateTimeFormat("en", {
      timeZone: zone,
      timeZoneName: "longOffset",
    })
      .formatToParts(now)
      .find((part) => part.type === "timeZoneName")?.value ?? "GMT"
  return JSON.stringify({
    workspace_time_zone: zone,
    utc_offset: offset,
    instruction: `Timestamps from tools are UTC. Say every date and time in the workspace time zone ${zone} (${offset}) and name that zone. Never use the device's, the server's or any other time zone, and never guess one.`,
  })
}
