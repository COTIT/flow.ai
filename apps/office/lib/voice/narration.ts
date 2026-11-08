import type { VoiceOutcome } from "./client-contract"

/** The agent's spoken review line, per session language. */
const review = {
  en: "Here is what I'll do. Please read and approve the card.",
  nl: "Dit ga ik doen. Lees de kaart en keur hem goed.",
  sw: "Hiki ndicho nitakachofanya. Tafadhali soma kadi na uidhinishe.",
}

/** Steps are message keys (`voice.steps.*`); Office shows their localized label, never catalog text. */
export function narrate(
  outcome: VoiceOutcome,
  stage: "lookup" | "review" | "execute",
  language = "en"
): VoiceOutcome {
  if (stage === "execute")
    return { ...outcome, steps: ["running", "finalizing"] }
  if (stage === "lookup") return { ...outcome, steps: ["checking"] }
  return {
    ...outcome,
    steps: ["checking", "preparing"],
    narration: review[language as keyof typeof review] ?? review.en,
  }
}
