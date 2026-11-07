import assert from "node:assert/strict"

// Run with Node --env-file=.env.local from Office; never print credentials or signed URLs.
const key = process.env.ELEVENLABS_API_KEY
const agent = process.env.ELEVENLABS_AGENT_ID
assert(key && agent, "Configure ElevenLabs runtime credentials first")
async function request(path, method = "GET") {
  const response = await fetch("https://api.elevenlabs.io" + path, { method, headers: { "xi-api-key": key }, redirect: "error", signal: AbortSignal.timeout(30000) })
  assert(response.ok, `Provider ${method} returned ${response.status}`)
  const raw = await response.text()
  return raw ? JSON.parse(raw) : {}
}
if (process.argv[2] === "models") {
  const catalog = await request("/v1/convai/llm/list")
  const models = Array.isArray(catalog) ? catalog : catalog.llms ?? catalog.models ?? []
  console.log(JSON.stringify({ keys: Object.keys(catalog), models: models.filter((m) => m.supports_image_input && m.supports_document_input).map((m) => ({ id: m.model_id ?? m.llm ?? m.id, name: m.name, image: m.supports_image_input, pdf: m.supports_document_input, pricing: m.pricing })) }, null, 2))
} else if (process.argv[2] === "session") {
  const session = await request(`/v1/convai/conversation/get-signed-url?agent_id=${encodeURIComponent(agent)}&include_conversation_id=true`)
  assert(session.signed_url, "Signed session has no URL")
  const url = new URL(session.signed_url)
  console.log(JSON.stringify({ responseKeys: Object.keys(session), queryKeys: [...url.searchParams.keys()] }))
  const conversationId = session.conversation_id ?? url.searchParams.get("conversation_id")
  assert(conversationId, "Signed session has no server-allocated conversation ID")
  assert(url.protocol === "wss:" && url.hostname === "api.elevenlabs.io")
  let deleted = false
  try { await request(`/v1/convai/conversations/${encodeURIComponent(conversationId)}`, "DELETE"); deleted = true }
  catch (error) { console.log(JSON.stringify({ cleanup: "allocation never connected; deletion may require conversation start", error: error.message })) }
  console.log(JSON.stringify({ signedSessionIssued: true, conversationIdAllocated: true, deleted }))
} else throw new Error("Choose models or session check")
