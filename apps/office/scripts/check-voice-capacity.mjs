import assert from "node:assert/strict"
import { spawn } from "node:child_process"
import { randomBytes, randomUUID } from "node:crypto"
import { chmod, mkdtemp, readFile, rm, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { fileURLToPath } from "node:url"
import { parseEnv } from "node:util"
import { deflateSync } from "node:zlib"
import { createServer, request as httpRequest } from "node:http"

/** Refuse shared databases, remote APIs and mutable images before starting any test. */
export function capacityPlan(input) {
  assert(
    /^sha256:[a-f0-9]{64}$/.test(input.image),
    "Pass an immutable local image ID"
  )
  const api = new URL(input.apiUrl)
  assert(
    api.protocol === "http:" &&
      api.hostname === "127.0.0.1" &&
      !api.username &&
      !api.password &&
      api.pathname === "/v1" &&
      !api.search &&
      !api.hash,
    "Capacity checks require a loopback API"
  )
  const database = new URL(input.databaseUrl)
  const checkpoint = new URL(input.checkpointUrl)
  assert(
    ["postgres:", "postgresql:"].includes(database.protocol) &&
      database.hostname === "127.0.0.1" &&
      /^\/genesy_assistant_checks_[a-zA-Z0-9_]+$/.test(database.pathname),
    "Capacity checks require an isolated local database"
  )
  assert(
    checkpoint.hostname === database.hostname &&
      checkpoint.port === database.port &&
      checkpoint.pathname === database.pathname &&
      checkpoint.username === "assistant_checkpoint_local",
    "Use the same database's restricted checkpoint login"
  )
  assert(
    Number.isInteger(input.basePort) &&
      input.basePort >= 1024 &&
      input.basePort <= 65534,
    "Invalid loopback port"
  )
  return { ...input, limits: [384, 512] }
}

/** Hold one exact admitted response after Rust commits; never change a request. */
export async function createFaultProxy(apiOrigin, port = 0) {
  const origin = new URL(apiOrigin)
  assert(
    origin.protocol === "http:" &&
      origin.hostname === "127.0.0.1" &&
      origin.pathname === "/" &&
      !origin.username &&
      !origin.password,
    "Fault proxy requires a fixed loopback origin"
  )
  let armed
  const held = new Set()
  const server = createServer((incoming, response) => {
    if (!incoming.url?.startsWith("/") || incoming.url.startsWith("//")) {
      response.writeHead(400).end()
      return
    }
    const upstream = httpRequest(
      apiOrigin + incoming.url,
      { method: incoming.method, headers: incoming.headers },
      (reply) => {
        const fault = armed
        if (
          fault &&
          incoming.method === "POST" &&
          incoming.headers["assistant-action-id"] === fault.actionId
        ) {
          armed = undefined
          fault.response = response
          held.add(response)
          response.once("close", () => held.delete(response))
          reply.once("end", () =>
            reply.statusCode >= 200 && reply.statusCode < 300
              ? fault.resolve()
              : fault.reject(
                  new Error("Armed operation did not commit successfully")
                )
          )
          reply.once("error", () =>
            fault.reject(new Error("Armed upstream response failed"))
          )
          reply.resume()
          return
        }
        response.writeHead(reply.statusCode, reply.headers)
        reply.pipe(response)
      }
    )
    upstream.on("error", () => response.destroy())
    incoming.pipe(upstream)
  })
  await new Promise((resolve, reject) => {
    server.once("error", reject)
    server.listen(port, "127.0.0.1", resolve)
  })
  return {
    port: server.address().port,
    arm: (actionId) => {
      assert(!armed, "Only one acknowledgement fault may be armed")
      const fault = { actionId }
      fault.committed = new Promise((resolve, reject) => {
        fault.resolve = resolve
        fault.reject = reject
      })
      fault.committed.catch(() => {})
      fault.release = () => fault.response?.destroy()
      armed = fault
      return { committed: fault.committed, release: fault.release }
    },
    close: () =>
      new Promise((resolve) => {
        for (const response of held) response.destroy()
        server.closeAllConnections()
        server.close(resolve)
      }),
  }
}

function docker(args, { tolerate = false } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn("docker", ["--context", "desktop-linux", ...args], {
      stdio: ["ignore", "pipe", "pipe"],
    })
    const chunks = []
    child.stdout.on("data", (chunk) => chunks.push(chunk))
    child.stderr.on("data", () => {})
    child.on("error", () => reject(new Error("Local Docker could not start")))
    child.on("close", (code) =>
      code === 0 || tolerate
        ? resolve(Buffer.concat(chunks).toString().trim())
        : reject(
            new Error(
              `Local Docker ${args[0]} failed (${code}); private runtime configuration was not logged`
            )
          )
    )
  })
}

export { docker as localDocker }

function pngChunk(name, data) {
  const type = Buffer.from(name)
  const bytes = Buffer.concat([type, data])
  let crc = 0xffffffff
  for (const byte of bytes) {
    crc ^= byte
    for (let bit = 0; bit < 8; bit++)
      crc = crc & 1 ? (crc >>> 1) ^ 0xedb88320 : crc >>> 1
  }
  const size = Buffer.alloc(4),
    checksum = Buffer.alloc(4)
  size.writeUInt32BE(data.length)
  checksum.writeUInt32BE((crc ^ 0xffffffff) >>> 0)
  return Buffer.concat([size, bytes, checksum])
}

/** A real incompressible RGBA image exercises buffers near the ten-MiB limit. */
export function capacityImage() {
  const width = 1536,
    height = 1536,
    stride = width * 4 + 1
  const pixels = randomBytes(stride * height)
  for (let row = 0; row < height; row++) pixels[row * stride] = 0
  const header = Buffer.alloc(13)
  header.writeUInt32BE(width, 0)
  header.writeUInt32BE(height, 4)
  header[8] = 8
  header[9] = 6
  const png = Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    pngChunk("IHDR", header),
    pngChunk("IDAT", deflateSync(pixels)),
    pngChunk("IEND", Buffer.alloc(0)),
  ])
  assert(png.length > 9 * 1024 * 1024 && png.length <= 10 * 1024 * 1024)
  return png
}

async function until(check, message, milliseconds = 45000) {
  const end = Date.now() + milliseconds
  do {
    const result = await check().catch(() => undefined)
    if (result) return result
    await new Promise((resolve) => setTimeout(resolve, 100))
  } while (Date.now() < end)
  throw new Error(message)
}

/** Uses only the explicitly verified local fixture and an already-built immutable image. */
export async function runCapacity(image, authPath, outputPath) {
  const office = parseEnv(
    await readFile(new URL("../.env.local", import.meta.url), "utf8")
  )
  const api = parseEnv(
    await readFile(new URL("../../api/.env", import.meta.url), "utf8")
  )
  const plan = capacityPlan({
    image,
    apiUrl: "http://127.0.0.1:3280/v1",
    databaseUrl: api.DATABASE_URL,
    checkpointUrl: office.ASSISTANT_CHECKPOINT_URL,
    basePort: 3290,
  })
  const auth = JSON.parse(await readFile(authPath, "utf8"))
  for (const key of ["workspaceId", "productId", "currencyId", "warehouseId"])
    assert(
      /^[a-f0-9-]{36}$/i.test(auth[key]),
      "Use the isolated capacity fixture"
    )
  assert(
    typeof auth.token === "string" && auth.token.length >= 20,
    "Missing private fixture login"
  )
  const { default: pg } = await import("pg")
  const { Conversation } = await import("@elevenlabs/client")
  const { PostgresSaver } =
    await import("@langchain/langgraph-checkpoint-postgres")
  const db = new pg.Client({ connectionString: plan.databaseUrl })
  const runtime = new pg.Pool({ connectionString: plan.checkpointUrl, max: 1 })
  const privateDir = await mkdtemp(join(tmpdir(), "genesy-voice-capacity-"))
  await chmod(privateDir, 0o700)
  const names = [],
    sessions = []
  const monitors = new Set()
  let proxy
  const evidence = { image, platform: "linux", memory: [], passed: false }
  try {
    await db.connect()
    const database = await db.query("SELECT current_database() AS name")
    assert(database.rows[0].name.startsWith("genesy_assistant_checks_"))
    const fixture = await db.query(
      "SELECT p.id FROM products p JOIN currencies c ON c.workspace_id=p.workspace_id AND c.id=p.currency_id WHERE p.workspace_id=$1 AND p.id=$2 AND c.id=$3 AND p.has_stock=false",
      [auth.workspaceId, auth.productId, auth.currencyId]
    )
    assert.equal(
      fixture.rowCount,
      1,
      "Capacity fixture must be a synthetic untracked product"
    )
    proxy = await createFaultProxy(new URL(plan.apiUrl).origin, 3281)
    const checkpoint = new URL(plan.checkpointUrl)
    checkpoint.hostname = "host.docker.internal"
    const secrets = {
      ELEVENLABS_API_KEY: office.ELEVENLABS_API_KEY,
      ASSISTANT_BRIDGE_SECRET: office.ASSISTANT_BRIDGE_SECRET,
      ASSISTANT_CHECKPOINT_URL: checkpoint.href,
    }
    const secretArgs = []
    for (const [key, value] of Object.entries(secrets)) {
      assert(
        value && !value.includes("\n"),
        "Configure private development runtime secrets"
      )
      const path = join(privateDir, key.toLowerCase())
      await writeFile(path, value, { mode: 0o444 })
      secretArgs.push(
        "-v",
        `${path}:/run/secrets/${key.toLowerCase()}:ro`,
        "-e",
        `${key}_FILE=/run/secrets/${key.toLowerCase()}`
      )
    }
    async function start(limit, index) {
      const name = `genesy-voice-capacity-${randomUUID()}`
      const port = plan.basePort + index
      names.push(name)
      await docker([
        "run",
        "-d",
        "--pull=never",
        "--name",
        name,
        "--label",
        "genesy-assistant-capacity=1",
        "--memory",
        `${limit}m`,
        "--memory-swap",
        `${limit}m`,
        "--cpus",
        "2",
        "--pids-limit",
        "200",
        "--stop-timeout",
        "30",
        "-p",
        `127.0.0.1:${port}:3000`,
        "-e",
        "ASSISTANT_ENABLED=true",
        "-e",
        `ASSISTANT_WORKSPACE_ALLOWLIST=${auth.workspaceId}`,
        "-e",
        `ELEVENLABS_AGENT_ID=${office.ELEVENLABS_AGENT_ID}`,
        "-e",
        `API_URL=http://host.docker.internal:${proxy.port}/v1`,
        "-e",
        "LANGSMITH_TRACING=false",
        ...secretArgs,
        image,
      ])
      const origin = `http://127.0.0.1:${port}`
      await until(
        async () => (await fetch(origin + "/healthz")).status === 200,
        "Office container did not become ready"
      )
      const readiness = await fetch(origin + "/api/voice/readiness")
      assert.equal(
        readiness.status,
        200,
        "Enabled container readiness must pass"
      )
      return { name, origin }
    }
    async function request(origin, endpoint, input, session, method = "POST") {
      const response = await fetch(origin + "/api/voice/" + endpoint, {
        method,
        headers: {
          Authorization: "Bearer " + auth.token,
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          workspaceId: auth.workspaceId,
          ...(session ? { sessionId: session.sessionId } : {}),
          ...input,
        }),
        signal: AbortSignal.timeout(45000),
      })
      const value = await response.json()
      return { response, value }
    }
    function orderInput(reference) {
      return {
        command: "execute",
        operationId: "orders_create",
        arguments: {
          body: {
            reference,
            currency_id: auth.currencyId,
            warehouse_id: auth.warehouseId,
            lines: [
              { product_id: auth.productId, quantity: "1", unit_price: 125 },
            ],
          },
        },
      }
    }
    const png = capacityImage()
    for (const limit of plan.limits) {
      const pair = await Promise.all([start(limit, 0), start(limit, 1)])
      const local = []
      for (const server of pair) {
        const issued = await request(server.origin, "session", { locale: "en" })
        assert.equal(issued.response.status, 200)
        const session = issued.value
        const conversation = await Conversation.startSession({
          signedUrl: session.signedUrl,
          connectionType: "websocket",
          textOnly: true,
          overrides: { agent: { language: "en" } },
          onConnect: ({ conversationId }) =>
            assert.equal(conversationId, session.conversationId),
          clientTools: {},
        })
        const entry = { ...server, session, conversation }
        local.push(entry)
        sessions.push(entry)
      }
      const tasks = [...local]
      for (const server of local) {
        const extra = await request(server.origin, "session", { locale: "en" })
        assert.equal(extra.response.status, 200)
        const entry = {
          ...server,
          session: extra.value,
          conversation: undefined,
        }
        tasks.push(entry)
        sessions.push(entry)
      }
      let checkpointPeak = 0,
        checking = false,
        monitorFailed = false
      const monitor = setInterval(() => {
        if (checking) return
        checking = true
        db.query(
          "SELECT count(*)::int AS n FROM pg_stat_activity WHERE usename='assistant_checkpoint_local'"
        )
          .then((result) => {
            checkpointPeak = Math.max(checkpointPeak, result.rows[0].n)
          })
          .catch(() => {
            monitorFailed = true
          })
          .finally(() => {
            checking = false
          })
      }, 20)
      monitors.add(monitor)
      const prepared = await Promise.all(
        tasks.map(async (server) => {
          const proposal = await request(
            server.origin,
            "tools",
            orderInput(`capacity-pool-${randomUUID()}`),
            server.session
          )
          assert.equal(
            proposal.response.status,
            200,
            "Both graph writers must finish with spare saver connections"
          )
          assert.equal(proposal.value.status, "requires_confirmation")
          return { server, ticket: proposal.value.review.ticket }
        })
      )
      const statuses = await Promise.all(
        local.flatMap((server) =>
          Array.from(
            { length: 8 },
            async () =>
              (
                await request(
                  server.origin,
                  "tools",
                  {
                    command: "execute",
                    operationId: "products_get",
                    arguments: { path: { product: auth.productId } },
                  },
                  server.session
                )
              ).response.status
          )
        )
      )
      assert(
        statuses.every((status) => [200, 409, 429].includes(status)) &&
          statuses.includes(200),
        "Read load must finish or refuse bounded capacity"
      )
      const uploadStatuses = await Promise.all(
        local.flatMap((server) =>
          Array.from({ length: 2 }, async () => {
            const response = await fetch(
              server.origin + "/api/voice/attachments",
              {
                method: "POST",
                headers: {
                  Authorization: "Bearer " + auth.token,
                  "X-Workspace-Id": auth.workspaceId,
                  "X-Voice-Session": server.session.sessionId,
                  "Content-Type": "image/png",
                  "Content-Length": String(png.length),
                },
                body: png,
                signal: AbortSignal.timeout(60000),
              }
            )
            await response.arrayBuffer()
            return response.status
          })
        )
      )
      assert(
        uploadStatuses.every((status) => [200, 201, 429].includes(status)) &&
          uploadStatuses.some((status) => status < 300),
        "Large valid uploads must succeed or refuse bounded capacity"
      )
      clearInterval(monitor)
      monitors.delete(monitor)
      await db.query("SELECT 1")
      assert(
        !monitorFailed && checkpointPeak >= 4 && checkpointPeak <= 10,
        "Blue/green graph writers did not exercise the bounded role pool"
      )
      for (const proposal of prepared) {
        const cancelled = await request(
          proposal.server.origin,
          "confirm",
          { ticket: proposal.ticket, decision: "cancel" },
          proposal.server.session
        )
        assert.equal(cancelled.response.status, 200)
        assert.equal(cancelled.value.status, "cancelled")
      }
      const before = await Promise.all(
        pair.map(async (server) => ({
          name: server.name,
          memory: await docker([
            "stats",
            "--no-stream",
            "--format",
            "{{.MemUsage}}",
            server.name,
          ]),
          peakBytes: Number(
            await docker([
              "exec",
              server.name,
              "cat",
              "/sys/fs/cgroup/memory.peak",
            ])
          ),
          memoryMax: Number(
            await docker([
              "exec",
              server.name,
              "cat",
              "/sys/fs/cgroup/memory.max",
            ])
          ),
          state: JSON.parse(
            await docker([
              "inspect",
              "--format",
              "{{json .State}}",
              server.name,
            ])
          ),
        }))
      )
      assert(
        before.every(
          (server) =>
            server.state.Running &&
            !server.state.OOMKilled &&
            server.state.ExitCode === 0
        ),
        "Office failed under bounded concurrency"
      )
      assert(
        before.every(
          (server) =>
            server.memoryMax === limit * 1024 * 1024 && server.peakBytes > 0
        ),
        "Container memory ceiling was not enforced"
      )
      const faults = []
      for (const [index, phase] of [
        "admitted_before_commit",
        "committed_before_acknowledgement",
      ].entries()) {
        const target = local[index],
          reference = `capacity-${randomUUID()}`
        const proposal = await request(
          target.origin,
          "tools",
          orderInput(reference),
          target.session
        )
        assert.equal(proposal.response.status, 200)
        assert.equal(proposal.value.status, "requires_confirmation")
        assert.equal(proposal.value.review.details.changes.reference, reference)
        assert.deepEqual(
          proposal.value.review.details.changes.lines,
          [{ product_id: auth.productId, quantity: "1", unit_price: "€1.25" }],
          "Only the intended synthetic order may be approved"
        )
        const actionId = proposal.value.data.actionId
        let fault
        if (phase === "admitted_before_commit") {
          await db.query("BEGIN")
          await db.query(
            "SELECT id FROM products WHERE workspace_id=$1 AND id=$2 FOR UPDATE",
            [auth.workspaceId, auth.productId]
          )
        } else fault = proxy.arm(actionId)
        const confirmation = request(
          target.origin,
          "confirm",
          {
            ticket: proposal.value.review.ticket,
            decision: "approve",
            method: "click",
          },
          target.session
        ).catch(() => undefined)
        const observer = new pg.Client({ connectionString: plan.databaseUrl })
        await observer.connect()
        try {
          if (fault)
            await Promise.race([
              fault.committed,
              new Promise((_, reject) => {
                const timeout = setTimeout(
                  () =>
                    reject(
                      new Error(
                        "Postcommit acknowledgement was not intercepted"
                      )
                    ),
                  20000
                )
                timeout.unref()
              }),
            ])
          else
            await until(
              async () =>
                (
                  await observer.query(
                    "SELECT state FROM assistant_actions WHERE workspace_id=$1 AND id=$2",
                    [auth.workspaceId, actionId]
                  )
                ).rows[0]?.state === "dispatching",
              "Write was not admitted before shutdown",
              15000
            )
          await docker(["stop", "--time", "1", target.name])
        } finally {
          fault?.release()
          await db.query("ROLLBACK")
          await observer.end()
        }
        await confirmation
        await until(
          async () =>
            (
              await db.query(
                "SELECT count(*)::int AS n FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock'"
              )
            ).rows[0].n === 0,
          "Write did not leave its held lock"
        )
        await new Promise((resolve) => setTimeout(resolve, 1000))
        const countBefore = await db.query(
          "SELECT count(*)::int AS n FROM orders WHERE workspace_id=$1 AND reference=$2",
          [auth.workspaceId, reference]
        )
        assert(
          countBefore.rows[0].n <= 1,
          "Shutdown duplicated a business write"
        )
        if (fault)
          assert.equal(
            countBefore.rows[0].n,
            1,
            "Postcommit fault must retain the committed business row"
          )
        await docker(["start", target.name])
        await until(
          async () => (await fetch(target.origin + "/healthz")).status === 200,
          "Restart failed"
        )
        const repeated = await request(
          target.origin,
          "confirm",
          {
            ticket: proposal.value.review.ticket,
            decision: "approve",
            method: "click",
          },
          target.session
        )
        assert.equal(repeated.response.status, 200)
        assert(
          ["running", "unknown", "completed", "failed"].includes(
            repeated.value.status
          ),
          "Restart must report recorded state"
        )
        if (fault)
          assert.equal(
            repeated.value.status,
            "completed",
            "Lost acknowledgement must recover the committed Rust receipt"
          )
        const countAfter = await db.query(
          "SELECT count(*)::int AS n FROM orders WHERE workspace_id=$1 AND reference=$2",
          [auth.workspaceId, reference]
        )
        assert.equal(
          countAfter.rows[0].n,
          countBefore.rows[0].n,
          "Restart replayed an admitted write"
        )
        const saver = new PostgresSaver(runtime, undefined, {
          schema: "assistant_checkpoint",
        })
        const saved = await saver.getTuple({
          configurable: { thread_id: target.session.sessionId },
        })
        assert(
          saved &&
            JSON.stringify(saved.checkpoint.channel_values).includes(actionId),
          "Checkpoint progress was lost after process replacement"
        )
        faults.push({
          phase,
          businessRows: countAfter.rows[0].n,
          restartStatus: repeated.value.status,
          checkpointSurvived: true,
          noDuplicateWrite: true,
        })
      }
      const final = await Promise.all(
        pair.map(async (server) =>
          JSON.parse(
            await docker([
              "inspect",
              "--format",
              "{{json .State}}",
              server.name,
            ])
          )
        )
      )
      assert(
        final.every((state) => state.Running && !state.OOMKilled),
        "Container did not survive the restart test"
      )
      evidence.memory.push({
        limitMiB: limit,
        instances: 2,
        readStatuses: statuses,
        uploads: { bytesEach: png.length, statuses: uploadStatuses },
        checkpointConnectionsPeak: checkpointPeak,
        memory: before.map((server) => ({
          usage: server.memory,
          peakBytes: server.peakBytes,
          limitBytes: server.memoryMax,
        })),
        faults,
        oomKilled: false,
      })
      for (const server of tasks) {
        await server.conversation?.endSession()
        const ended = await request(
          server.origin,
          "session",
          {},
          server.session,
          "DELETE"
        )
        assert.equal(ended.response.status, 200)
      }
      for (const server of pair) await docker(["rm", "-f", server.name])
    }
    evidence.passed = true
    await writeFile(outputPath, JSON.stringify(evidence, null, 2) + "\n", {
      mode: 0o600,
    })
    console.log(
      JSON.stringify({
        capacityPassed: true,
        limitsMiB: plan.limits,
        instancesPerLimit: 2,
        image,
        evidence: outputPath,
      })
    )
  } finally {
    for (const monitor of monitors) clearInterval(monitor)
    await db.query("ROLLBACK").catch(() => {})
    for (const session of sessions) {
      await session.conversation?.endSession().catch(() => {})
      await fetch(
        `${plan.apiUrl}/workspace/${encodeURIComponent(auth.workspaceId)}/assistant/sessions/${encodeURIComponent(session.session.sessionId)}`,
        {
          method: "DELETE",
          headers: { Authorization: "Bearer " + auth.token },
          signal: AbortSignal.timeout(5000),
        }
      )
        .then((response) => {
          if (![200, 204, 404].includes(response.status))
            console.error(
              "Owned capacity session cleanup needs operator verification"
            )
        })
        .catch(() =>
          console.error(
            "Owned capacity session cleanup needs operator verification"
          )
        )
    }
    for (const name of names)
      await docker(["rm", "-f", name], { tolerate: true })
    await proxy?.close()
    await runtime.end()
    await db.end()
    await rm(privateDir, { recursive: true, force: true })
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  assert(
    process.argv.length === 5,
    "Usage: node scripts/check-voice-capacity.mjs sha256:IMAGE /private/auth.json /private/evidence.json"
  )
  await runCapacity(...process.argv.slice(2))
}
