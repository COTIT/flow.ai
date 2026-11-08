import type { VoiceOutcome } from "./client-contract"

/** Status-line labels and the agent's review line, per session language. */
const phrases = {
  en: {
    checking: "Checking: ",
    preparing: "Preparing your review",
    running: "Running it now",
    finalizing: "Finalizing",
    review: "Here is what I'll do. Please read and approve the card.",
  },
  nl: {
    checking: "Controleren: ",
    preparing: "Je controle voorbereiden",
    running: "Bezig met uitvoeren",
    finalizing: "Afronden",
    review: "Dit ga ik doen. Lees de kaart en keur hem goed.",
  },
  sw: {
    checking: "Inakagua: ",
    preparing: "Inaandaa ukaguzi wako",
    running: "Inaendesha sasa",
    finalizing: "Inakamilisha",
    review: "Hiki ndicho nitakachofanya. Tafadhali soma kadi na uidhinishe.",
  },
}

export function narrate(
  outcome: VoiceOutcome,
  stage: "lookup" | "review" | "execute",
  summary: string,
  language = "en"
): VoiceOutcome {
  const text = phrases[language as keyof typeof phrases] ?? phrases.en
  if (stage === "execute")
    return { ...outcome, steps: [text.running, text.finalizing] }
  if (stage === "lookup")
    return { ...outcome, steps: [text.checking + summary] }
  return {
    ...outcome,
    steps: [text.checking + summary, text.preparing],
    narration: text.review,
  }
}
