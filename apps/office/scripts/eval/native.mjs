import assert from "node:assert/strict"
import { isDeepStrictEqual, parseEnv } from "node:util"
import { createHash, createHmac, randomUUID } from "node:crypto"
import { readFile } from "node:fs/promises"
import { spawn } from "node:child_process"
import { fileURLToPath } from "node:url"
import { localDocker } from "../check-voice-capacity.mjs"

const CASES = {
  "product-create": { test: "products::create_writes_default_variant_and_price_history", operation: "products_create", body: { name: "A", price: "220", gross_price: "200" } },
  "cash-invoice-payment": { test: "invoice_payments::cash_payments_settle_an_invoice_within_five_cents", operation: "invoice_payments_create", body: { date: "2026-10-23", amount: "60", payment_method: "cash" } },
}

/** Validate the ephemeral synthetic-test envelope before any vendor or Docker request. */
export function nativePlan(input, image) {
  assert(/^sha256:[a-f0-9]{64}$/.test(image), "An immutable local Office image is required")
  assert(input?.formatVersion === 1 && Object.hasOwn(CASES, input.caseId), "Unknown reviewed native case")
  const reviewed = CASES[input.caseId]
  assert(input.sourceTest === reviewed.test, "Native fixture source differs")
  const origin = new URL(input.apiOrigin)
  assert(origin.protocol === "http:" && origin.hostname === "127.0.0.1" && Number(origin.port) >= 1024 && origin.pathname === "/" && !origin.username && !origin.password && !origin.search && !origin.hash, "Native fixtures require a loopback API")
  assert(/^_sqlx_test_[a-zA-Z0-9_]{52}$/.test(input.databaseName), "Native fixture database must be disposable SQLx state")
  assert(/^[a-f0-9-]{36}$/i.test(input.workspaceId) && /^[a-f0-9]{64}$/i.test(input.bearer), "Invalid synthetic fixture identity")
  assert(input.testBridgeSecret === "genesy-native-reviewed-test-bridge-key-2026", "Only the public test bridge key is accepted")
  const request = input.request
  assert(request && Object.keys(request).sort().join(",") === "body,method,path" && request.method === "POST" && isDeepStrictEqual(request.body, reviewed.body), "Native fixture request differs from reviewed intent")
  const prefix = `/v1/workspace/${input.workspaceId}`
  assert(input.caseId === "product-create" ? request.path === prefix + "/products" : new RegExp(`^${prefix}/invoices/[a-f0-9-]{36}/payments$`, "i").test(request.path), "Native fixture target differs")
  const language = input.language ?? "en", repetition = input.repetition ?? 1
  assert(["en", "nl", "sw"].includes(language) && [1, 2, 3].includes(repetition), "Invalid native trial slot")
  const path = input.caseId === "cash-invoice-payment" ? { invoice: request.path.split("/")[5] } : undefined
  return { ...input, image, operationId: reviewed.operation, language, repetition, arguments: { ...(path ? { path } : {}), body: reviewed.body } }
}

/** The application supplies a private card; the native model receives no challenge or ticket. */
export function nativeReviewReply(value) {
  assert(value.status === "requires_confirmation" && typeof value.data?.actionId === "string")
  return { status: value.status, actionId: value.data.actionId, message: "The user must inspect and independently approve the current Office card. Then wait for assistant_task_status with this actionId and wait:true." }
}

/** Approve only the exact source-reviewed request; no inferred scope reaches the handler. */
export function verifyNativeCard(value, plan) {
  const review = value.review
  assert(value.status === "requires_confirmation" && review && !review.batchId, "Expected one immutable individual review")
  assert(review.details?.workspace === plan.workspaceId && isDeepStrictEqual(review.details?.changes, plan.request.body), "Canonical review changed the intended request")
  assert(isDeepStrictEqual(review.details.target, plan.arguments.path ?? {}), "Canonical review changed its target")
  assert(typeof review.ticket === "string" && review.ticket.length > 20 && Date.parse(review.expiresAt) > Date.now(), "Private review is missing or expired")
  return review
}

async function retry(check, milliseconds = 15000) {
  const end = Date.now() + milliseconds
  while (Date.now() < end) {
    const value = await check().catch(() => undefined)
    if (value) return value
    await new Promise((resolve) => setTimeout(resolve, 100))
  }
  throw new Error("Native fixture deadline exceeded")
}

/** Run real provider/BFF/graph requests while the original SQLx fixture remains in RAM. */
export async function runNativeFixture(input, image) {
  const plan = nativePlan(input, image)
  const office = parseEnv(await readFile(new URL("../../.env.local", import.meta.url), "utf8"))
  const key = office.ELEVENLABS_API_KEY
  const agentId = process.env.GENESY_ASSISTANT_NATIVE_AGENT_ID || office.ELEVENLABS_AGENT_ID
  assert(key && /^[a-zA-Z0-9_-]{1,200}$/.test(agentId), "Configure the development native provider")
  async function vendor(path, method = "GET") {
    assert(path.startsWith("/v1/convai/"))
    const response = await fetch("https://api.elevenlabs.io" + path, { method, headers: { "xi-api-key": key }, redirect: "error", signal: AbortSignal.timeout(10000) })
    if (response.status === 404) return { absent: true }
    assert(response.ok, "Native provider request failed")
    const bytes = await response.arrayBuffer()
    assert(bytes.byteLength < 2_000_000, "Unexpected provider metadata size")
    return bytes.byteLength ? JSON.parse(new TextDecoder().decode(bytes)) : {}
  }
  const configured = await vendor("/v1/convai/agents/" + agentId)
  assert(/genesy.*(development|evaluation|staging)/i.test(configured.name), "Use an isolated development agent")
  assert(typeof configured.version_id === "string")
  const model = configured.conversation_config?.agent?.prompt?.llm
  assert(typeof model === "string")
  const checkpoint = new URL(office.ASSISTANT_CHECKPOINT_URL)
  assert(["localhost", "127.0.0.1"].includes(checkpoint.hostname) && checkpoint.username === "assistant_checkpoint_local", "Use the existing restricted local checkpoint role")
  checkpoint.pathname = "/" + plan.databaseName
  const { default: pg } = await import("pg")
  const db = new pg.Client({ connectionString: checkpoint.href, connectionTimeoutMillis: 3000, statement_timeout: 3000 })
  await db.connect()
  try {
    const checks = await db.query("SELECT max(v)::int AS version FROM assistant_checkpoint.checkpoint_migrations")
    assert.equal(checks.rows[0].version, 5)
    await assert.rejects(db.query("SELECT id FROM public.workspaces LIMIT 1"), { code: "42501" })
    const ddl = await db.query("SELECT has_schema_privilege(current_user,'assistant_checkpoint','CREATE') AS allowed")
    assert.equal(ddl.rows[0].allowed, false)
  } finally { await db.end() }
  checkpoint.hostname = "host.docker.internal"
  const variables = {
    ASSISTANT_ENABLED: "true",
    ASSISTANT_WORKSPACE_ALLOWLIST: plan.workspaceId,
    API_URL: `http://host.docker.internal:${new URL(plan.apiOrigin).port}/v1`,
    ASSISTANT_BRIDGE_SECRET: plan.testBridgeSecret,
    ASSISTANT_CHECKPOINT_URL: checkpoint.href,
    ELEVENLABS_API_KEY: key,
    ELEVENLABS_AGENT_ID: agentId,
    LANGSMITH_TRACING: "false",
  }
  const bootstrap = `const r=require('node:readline').createInterface({input:process.stdin});r.once('line',line=>{try{const values=JSON.parse(line);for(const name of ${JSON.stringify(Object.keys(variables))}){if(typeof values[name]!=='string')throw Error();process.env[name]=values[name]}r.close();require('./server.js')}catch{process.stderr.write('Native fixture bootstrap refused\\n');process.exit(1)}})`
  const name = "genesy-native-fixture-" + randomUUID()
  let conversation, session, origin, result, failure
  const started = Date.now()
  let completed = false, cleanupVerified = false, finalResult, review, actionId, preparing = false, statusDelivered = false
  let agentMessages = 0, lastReply = "", toolCalls = 0
  let finishReview, finishResult, finishReply, rejectNative
  const prepared = new Promise((resolve) => { finishReview = resolve })
  const outcome = new Promise((resolve) => { finishResult = resolve })
  const replied = new Promise((resolve) => { finishReply = resolve })
  const failed = new Promise((_, reject) => { rejectNative = reject })
  failed.catch(() => {})
  const wait = (promise, ms) => Promise.race([promise, failed, new Promise((_, reject) => { const timer = setTimeout(() => reject(new Error("Native task timed out")), ms); timer.unref() })])
  async function request(path, body, method = "POST") {
    const response = await fetch(origin + "/api/voice/" + path, { method, headers: { Authorization: "Bearer " + plan.bearer, "Content-Type": "application/json" }, body: JSON.stringify({ workspaceId: plan.workspaceId, ...(session ? { sessionId: session.sessionId } : {}), ...body }), signal: AbortSignal.timeout(30000) })
    assert(response.ok, "Native Office request failed")
    return response.json()
  }
  try {
    const child = spawn("docker", ["--context", "desktop-linux", "run", "--rm", "--pull=never", "--name", name, "--label", "genesy-native-fixture=1", "--memory", "512m", "--memory-swap", "512m", "--cpus", "2", "--pids-limit", "200", "-p", "127.0.0.1::3000", "-i", "--entrypoint", "node", image, "-e", bootstrap], { stdio: ["pipe", "ignore", "ignore"] })
    child.on("error", () => rejectNative(new Error("Native fixture container failed")))
    child.stdin.on("error", () => {})
    child.stdin.end(JSON.stringify(variables) + "\n")
    const published = await retry(() => localDocker(["port", name, "3000/tcp"]))
    assert(/^127\.0\.0\.1:\d+$/.test(published))
    origin = "http://" + published
    await retry(async () => (await fetch(origin + "/healthz", { signal: AbortSignal.timeout(2000) })).status === 200)
    const ready = await fetch(origin + "/api/voice/readiness")
    assert.equal(ready.status, 200)
    session = await request("session", { locale: plan.language, textOnly: true })
    const { Conversation } = await import("@elevenlabs/client")
    conversation = await Conversation.startSession({
      signedUrl: session.signedUrl, connectionType: "websocket", textOnly: true,
      overrides: { agent: { language: plan.language } },
      onConnect: ({ conversationId }) => { if (conversationId !== session.conversationId) rejectNative(new Error("Native identity mismatch")) },
      onError: () => rejectNative(new Error("Native conversation failed")),
      onMessage: (message) => {
        if (message.source !== "ai") return
        agentMessages++
        if (agentMessages > 12 || message.message.length > 4000) rejectNative(new Error("Native reply exceeded fixture bounds"))
        if (statusDelivered && finalResult) { lastReply = message.message; finishReply() }
      },
      clientTools: {
        discover_genesy_tools: async ({ query }) => JSON.stringify(await request("tools", { command: "discover", query })),
        execute_genesy_tool: async ({ operation_id, arguments_json }) => {
          try {
            assert(++toolCalls <= 12, "Native tool budget exceeded")
            const args = JSON.parse(arguments_json)
            if (operation_id === "assistant_task_status") {
              assert(actionId && args.actionId === actionId)
              if (args.wait === true) {
                await wait(outcome, 80000)
                statusDelivered = true
                return JSON.stringify(finalResult)
              }
              return JSON.stringify(await request("tools", { command: "status", actionId }))
            }
            assert(operation_id === plan.operationId && isDeepStrictEqual(args, plan.arguments), "Native operation changed the reviewed request")
            if (preparing || review) return JSON.stringify({ status: "failed", message: "Finish the existing review first." })
            preparing = true
            const value = await request("tools", { command: "execute", operationId: operation_id, arguments: args })
            review = verifyNativeCard(value, plan)
            actionId = value.data.actionId
            finishReview()
            return JSON.stringify(nativeReviewReply(value))
          } catch {
            rejectNative(new Error("Native fixture operation refused"))
            return JSON.stringify({ status: "failed", message: "This operation differs from the independently reviewed fixture. No retry is authorized." })
          } finally { preparing = false }
        },
      },
    })
    const dataset = JSON.parse(await readFile(new URL("./fixtures.json", import.meta.url), "utf8"))
    const fixture = dataset.cases.find((candidate) => candidate.nativeFixture?.caseId === plan.caseId)
    assert(fixture?.commands?.[plan.language], "Review native commands before running paid traffic")
    const command = fixture.commands[plan.language].replaceAll("{{invoiceId}}", plan.arguments.path?.invoice ?? "")
    conversation.sendUserMessage(command)
    await wait(prepared, 90000)
    finalResult = await request("confirm", { ticket: review.ticket, decision: "approve", method: "click" })
    assert.equal(finalResult.status, "completed")
    finishResult()
    await wait(replied, 30000)
    assert(plan.caseId === "product-create" ? /\bA\b/.test(lastReply) : /\b60(?:[.,]00)?\b/.test(lastReply) && /€|EUR|euro/i.test(lastReply), "Native reply omitted the verified identity or amount")
    completed = true
    const metadata = await vendor("/v1/convai/conversations/" + session.conversationId)
    const raw = metadata.metadata ?? {}
    const usage = { agentMessages, toolCalls }
    for (const field of ["cost_fiat", "call_duration_secs", "cost"]) if (typeof raw[field] === "number" && Number.isFinite(raw[field]) && raw[field] >= 0) usage[field] = raw[field]
    const unchanged = await vendor("/v1/convai/agents/" + agentId)
    assert.equal(unchanged.version_id, configured.version_id, "Agent changed during native validation")
    result = { completed, cleanupVerified: false, sessionId: session.sessionId, model, agentVersion: configured.version_id, graphVersion: image, language: plan.language, repetition: plan.repetition, latencyMs: Date.now() - started, usageSnapshot: "before_conversation_end", usage }
  } catch (error) { failure = error }
  finally {
    if (conversation) await conversation.endSession().catch(() => {})
    if (session) {
      try {
        await request("session", {}, "DELETE")
        await vendor("/v1/convai/conversations/" + session.conversationId, "DELETE")
        await retry(async () => (await vendor("/v1/convai/conversations/" + session.conversationId)).absent === true)
        const path = `/v1/assistant/cleanup/${session.sessionId}/ack`
        const timestamp = String(Math.floor(Date.now() / 1000))
        const signature = createHmac("sha256", plan.testBridgeSecret).update(`${timestamp}\nPOST\n${path}\n${createHash("sha256").update("").digest("hex")}`).digest("hex")
        const ack = await fetch(plan.apiOrigin + path, { method: "POST", headers: { "Assistant-Bridge-Timestamp": timestamp, "Assistant-Bridge-Signature": signature }, signal: AbortSignal.timeout(5000) })
        assert(ack.ok, "Native session cleanup was not acknowledged")
        cleanupVerified = true
      } catch { failure ??= new Error("Native fixture cleanup needs operator verification") }
    }
    await localDocker(["rm", "-f", name], { tolerate: true })
  }
  if (failure) throw failure
  assert(completed && cleanupVerified && result)
  return { ...result, cleanupVerified }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const output = process.stdout.write.bind(process.stdout)
  console.log = console.warn = console.error = console.debug = () => {}
  try {
    let wire = ""
    for await (const chunk of process.stdin) { wire += chunk; assert(Buffer.byteLength(wire) <= 131072, "Native fixture envelope too large") }
    const result = await runNativeFixture(JSON.parse(wire), process.env.ASSISTANT_NATIVE_IMAGE_ID)
    output(JSON.stringify(result) + "\n")
  } catch {
    process.stderr.write("Native fixture execution failed; no credentials or transcript logged.\n")
    process.exitCode = 1
  }
}
