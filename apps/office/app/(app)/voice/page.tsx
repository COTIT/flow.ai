import { VoiceConversation } from "@/components/voice/conversation"
import { availableCapabilities } from "@/lib/voice/capabilities.server"

export default async function VoicePage({
  searchParams,
}: {
  searchParams: Promise<{ from?: string | string[] }>
}) {
  const { from } = await searchParams
  return (
    <VoiceConversation
      capabilities={availableCapabilities()}
      from={typeof from === "string" ? from : undefined}
    />
  )
}
