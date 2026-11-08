import { notFound } from "next/navigation"
import { VoiceDesignPreview } from "@/components/voice/design-preview"

/** Living design mockup of the assistant screen; development builds only. */
export default function VoiceDesignPage() {
  if (process.env.NODE_ENV === "production") notFound()
  return <VoiceDesignPreview />
}
