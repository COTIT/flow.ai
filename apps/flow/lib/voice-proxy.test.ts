import { createHash, randomBytes } from "node:crypto"
import {
  createServer,
  type IncomingMessage,
  type Server,
  type ServerResponse,
} from "node:http"
import type { AddressInfo } from "node:net"
import {
  afterAll,
  afterEach,
  beforeAll,
  describe,
  expect,
  it,
  vi,
} from "vitest"

import { forwardVoice, UPSTREAM_BODY_LIMIT } from "./voice-proxy"

type Seen = {
  method: string
  url: string
  headers: IncomingMessage["headers"]
  bytes: number
  sha256: string
  complete: boolean
}
let reply: (req: IncomingMessage, res: ServerResponse) => void
let seen: Seen[] = []
let server: Server
let upstream = ""

beforeAll(async () => {
  server = createServer((req, res) => {
    const hash = createHash("sha256")
    const entry: Seen = {
      method: req.method!,
      url: req.url!,
      headers: req.headers,
      bytes: 0,
      sha256: "",
      complete: false,
    }
    seen.push(entry)
    req.on("data", (chunk: Buffer) => {
      entry.bytes += chunk.length
      hash.update(chunk)
    })
    req.on("end", () => {
      entry.complete = true
      entry.sha256 = hash.digest("hex")
      reply(req, res)
    })
  })
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve))
  upstream = `http://127.0.0.1:${(server.address() as AddressInfo).port}`
})
afterAll(() => new Promise<void>((resolve) => server.close(() => resolve())))
afterEach(() => {
  seen = []
  vi.restoreAllMocks()
})

const json =
  (status: number, body: unknown, headers = {}) =>
  (_req: IncomingMessage, res: ServerResponse) => {
    res.writeHead(status, { "Content-Type": "application/json", ...headers })
    res.end(JSON.stringify(body))
  }
const call = (
  route: string,
  init: RequestInit & { duplex?: "half" } = {},
  options: { upstream?: string; timeoutMs?: number } = {}
) =>
  forwardVoice(
    new Request(`http://flow.test/api/voice/${route}?x=1`, init),
    route,
    { upstream, ...options }
  )
const sha = (bytes: Uint8Array) =>
  createHash("sha256").update(bytes).digest("hex")
const BEARER = "Bearer secret-token-123"

describe("allowlist", () => {
  it("forwards history POST only, never the public webhook", async () => {
    reply = json(200, { conversations: [] })
    expect((await call("history", { method: "POST", body: "{}" })).status).toBe(200)
    expect(seen[0]?.url).toBe("/api/voice/history")
    expect((await call("history")).status).toBe(405)
    expect((await call("webhook", { method: "POST", body: "{}" })).status).toBe(404)
    expect(seen).toHaveLength(1)
  })

  it("forwards readiness to the fixed upstream path, status and body unchanged", async () => {
    reply = json(
      503,
      { enabled: false, ready: false },
      { "Cache-Control": "no-store" }
    )
    const response = await call("readiness")
    expect(response.status).toBe(503)
    expect(await response.json()).toEqual({ enabled: false, ready: false })
    expect(response.headers.get("cache-control")).toBe("no-store")
    expect(seen).toMatchObject([{ method: "GET", url: "/api/voice/readiness" }])
  })

  it.each(["secure", "unknown", "..", "readiness/..", "session%2F..", ""])(
    "refuses route %j with 404 JSON and never calls the upstream",
    async (route) => {
      const response = await call(route, { method: "POST", body: "{}" })
      expect(response.status).toBe(404)
      expect(await response.json()).toEqual({ message: expect.any(String) })
      expect(seen).toHaveLength(0)
    }
  )

  it("refuses a method the route does not take with 405 and Allow", async () => {
    const response = await call("session", { method: "GET" })
    expect(response.status).toBe(405)
    expect(response.headers.get("allow")).toBe("POST, DELETE")
    expect(await response.json()).toEqual({ message: expect.any(String) })
    expect(seen).toHaveLength(0)
  })
})

describe("request headers", () => {
  it("passes bearer, workspace, session, file name and content headers unchanged", async () => {
    reply = json(200, { id: "u1" })
    const body = randomBytes(1000)
    await call("attachments", {
      method: "POST",
      headers: {
        Authorization: BEARER,
        "X-Workspace-Id": "w1",
        "X-Voice-Session": "s1",
        "X-File-Name": "price%20list.png",
        "Content-Type": "image/png",
        "Content-Length": "1000",
        Accept: "application/json",
      },
      body,
    })
    expect(seen[0]!.headers).toMatchObject({
      authorization: BEARER,
      "x-workspace-id": "w1",
      "x-voice-session": "s1",
      "x-file-name": "price%20list.png",
      "content-type": "image/png",
      "content-length": "1000",
      accept: "application/json",
    })
  })

  it("drops cookies, host, forwarding, hop-by-hop and any credential the browser adds", async () => {
    reply = json(200, { status: "ended" })
    await call("session", {
      method: "DELETE",
      headers: {
        Authorization: BEARER,
        "Content-Type": "application/json",
        Cookie: "genesy_voice_gap=x; other=y",
        Host: "evil.test",
        "X-Forwarded-For": "1.2.3.4",
        "X-Forwarded-Host": "evil.test",
        "X-Forwarded-Proto": "http",
        Forwarded: "for=1.2.3.4",
        "X-Real-IP": "1.2.3.4",
        "X-Api-Key": "gsk_live",
        "Assistant-Bridge-Signature": "forged",
        "Assistant-Bridge-Timestamp": "1",
        "X-Nonce": "n",
        Connection: "x-secret",
        "X-Secret": "hop",
        "Keep-Alive": "timeout=5",
        TE: "trailers",
        Trailer: "X-Trailer",
        Upgrade: "websocket",
        "Proxy-Authorization": "Basic abc",
      },
      body: JSON.stringify({ workspaceId: "w1", sessionId: "s1" }),
    })
    const headers = seen[0]!.headers
    for (const name of [
      "cookie",
      "x-forwarded-for",
      "x-forwarded-host",
      "x-forwarded-proto",
      "forwarded",
      "x-real-ip",
      "x-api-key",
      "assistant-bridge-signature",
      "assistant-bridge-timestamp",
      "x-nonce",
      "x-secret",
      "keep-alive",
      "te",
      "trailer",
      "upgrade",
      "proxy-authorization",
    ])
      expect(headers, name).not.toHaveProperty(name)
    expect(headers.host).toBe(new URL(upstream).host)
    expect(headers.connection).not.toBe("x-secret")
    expect(headers.authorization).toBe(BEARER)
  })
})

describe("responses", () => {
  it.each([401, 403, 409, 413, 415, 422, 429, 500, 503])(
    "passes status %i and its JSON body through",
    async (status) => {
      reply = json(status, { message: `m${status}`, reason: "invalid" })
      const response = await call("tools", {
        method: "POST",
        headers: { Authorization: BEARER, "Content-Type": "application/json" },
        body: "{}",
      })
      expect(response.status).toBe(status)
      expect(await response.json()).toEqual({
        message: `m${status}`,
        reason: "invalid",
      })
    }
  )

  it("passes Office's session DELETE answer exactly", async () => {
    reply = json(200, { status: "ended" }, { "Cache-Control": "no-store" })
    const response = await call("session", {
      method: "DELETE",
      headers: { Authorization: BEARER, "Content-Type": "application/json" },
      body: JSON.stringify({ workspaceId: "w1", sessionId: "s1" }),
      keepalive: true,
    })
    expect(response.status).toBe(200)
    expect(await response.json()).toEqual({ status: "ended" })
    expect(seen[0]).toMatchObject({
      method: "DELETE",
      url: "/api/voice/session",
    })
  })

  it.each([204, 205])("passes an empty %i through", async (status) => {
    reply = (_req, res) => {
      res.writeHead(status)
      res.end()
    }
    const response = await call("session", { method: "DELETE", body: "{}" })
    expect(response.status).toBe(status)
    expect(await response.text()).toBe("")
  })

  it("passes an empty 200 with no content type through", async () => {
    reply = (_req, res) => {
      res.writeHead(200, { "Content-Length": "0" })
      res.end()
    }
    const response = await call("readiness")
    expect(response.status).toBe(200)
    expect(await response.text()).toBe("")
  })

  it("keeps only safe response headers: no Set-Cookie, hop-by-hop or server details", async () => {
    reply = json(
      429,
      { message: "busy" },
      {
        "Set-Cookie": "genesy_voice_gap=a.b.c; Path=/; HttpOnly",
        "Cache-Control": "no-store",
        "Retry-After": "5",
        "Keep-Alive": "timeout=5",
        "Proxy-Authenticate": "Basic",
        Trailer: "X-T",
        Upgrade: "h2c",
        "X-Powered-By": "Next.js",
        Server: "office",
        Location: "/elsewhere",
      }
    )
    const response = await call("tools", { method: "POST", body: "{}" })
    expect([...response.headers.keys()].sort()).toEqual([
      "cache-control",
      "content-type",
      "retry-after",
    ])
  })

  it.each([
    [
      "a redirect",
      (req: IncomingMessage, res: ServerResponse) => {
        // A followed redirect would land on a valid JSON answer; manual mode must not get there.
        if (req.url === "/landed")
          return json(200, { followed: true })(req, res)
        res.writeHead(302, { Location: `${upstream}/landed` })
        res.end()
      },
    ],
    [
      "an HTML page",
      (_req: IncomingMessage, res: ServerResponse) => {
        res.writeHead(200, { "Content-Type": "text/html" })
        res.end("<html>office</html>")
      },
    ],
  ])("turns %s into a generic 502", async (_name, handler) => {
    reply = handler
    // GET: a body-less request is the one fetch could follow a redirect with.
    const response = await call("readiness")
    expect(response.status).toBe(502)
    expect(response.headers.get("location")).toBeNull()
    expect(await response.json()).toEqual({ message: expect.any(String) })
    expect(seen).toHaveLength(1)
  })
})

describe("bodies", () => {
  it("streams a 5 MB binary upload byte for byte with its length", async () => {
    reply = json(200, { id: "u1" })
    const bytes = randomBytes(5 * 1024 * 1024)
    const response = await call("attachments", {
      method: "POST",
      headers: {
        "Content-Type": "application/pdf",
        "Content-Length": String(bytes.length),
      },
      body: bytes,
    })
    expect(response.status).toBe(200)
    expect(seen[0]).toMatchObject({ bytes: bytes.length, sha256: sha(bytes) })
    expect(seen[0]!.headers["content-length"]).toBe(String(bytes.length))
  })

  it("streams a multipart body unchanged, boundary included", async () => {
    reply = json(200, { status: "awaiting_review" })
    const form = new FormData()
    form.append("payload", JSON.stringify({ command: "execute" }))
    form.append(
      "0b9d4b6c-1d1b-4f3a-9d5c-2f9a3c1e7a10",
      new Blob([randomBytes(300_000)], { type: "image/jpeg" }),
      "photo.jpg"
    )
    const wire = new Request("http://x.test", { method: "POST", body: form })
    const type = wire.headers.get("content-type")!
    const bytes = new Uint8Array(await wire.arrayBuffer())
    await call("tools", {
      method: "POST",
      headers: { "Content-Type": type, "Content-Length": String(bytes.length) },
      body: bytes,
    })
    expect(seen[0]).toMatchObject({ bytes: bytes.length, sha256: sha(bytes) })
    expect(seen[0]!.headers["content-type"]).toBe(type)
  })

  it("streams a body with no declared length (chunked)", async () => {
    reply = json(200, { ok: true })
    const bytes = randomBytes(200_000)
    const response = await call("confirm", {
      method: "POST",
      body: new Blob([bytes]).stream(),
      duplex: "half",
    })
    expect(response.status).toBe(200)
    expect(seen[0]).toMatchObject({ bytes: bytes.length, sha256: sha(bytes) })
  })

  it("refuses a declared length over the cap with 413 before calling the upstream", async () => {
    const response = await call("attachments", {
      method: "POST",
      headers: { "Content-Length": String(UPSTREAM_BODY_LIMIT + 1) },
      body: new Blob([new Uint8Array(10)]).stream(),
      duplex: "half",
    })
    expect(response.status).toBe(413)
    expect(await response.json()).toEqual({ message: expect.any(String) })
    expect(seen).toHaveLength(0)
  })

  it("refuses a body longer than its declared length with 413", async () => {
    reply = json(200, {})
    const response = await call("attachments", {
      method: "POST",
      headers: { "Content-Length": "100" },
      body: new Blob([new Uint8Array(5000)]).stream(),
      duplex: "half",
    })
    expect(response.status).toBe(413)
    expect(seen.filter((s) => s.complete)).toHaveLength(0)
  })

  it("refuses a body shorter than its declared length with 413", async () => {
    reply = json(200, {})
    const response = await call("attachments", {
      method: "POST",
      headers: { "Content-Length": "5000" },
      body: new Blob([new Uint8Array(100)]).stream(),
      duplex: "half",
    })
    expect(response.status).toBe(413)
    expect(seen.filter((s) => s.complete)).toHaveLength(0)
  })

  it("stops an undeclared body at the cap with 413", async () => {
    reply = json(200, {})
    const chunk = new Uint8Array(1024 * 1024)
    let sent = 0
    const endless = new ReadableStream<Uint8Array>({
      pull(controller) {
        sent += chunk.length
        controller.enqueue(chunk)
      },
    })
    const response = await call("tools", {
      method: "POST",
      body: endless,
      duplex: "half",
    })
    expect(response.status).toBe(413)
    expect(sent).toBeLessThan(UPSTREAM_BODY_LIMIT + 4 * chunk.length)
    expect(seen.filter((s) => s.complete)).toHaveLength(0)
  })
})

describe("failures", () => {
  it("answers 502 without internal details when the upstream is down", async () => {
    const response = await call(
      "readiness",
      {},
      { upstream: "http://127.0.0.1:9" }
    )
    expect(response.status).toBe(502)
    const text = await response.text()
    expect(JSON.parse(text)).toEqual({ message: expect.any(String) })
    expect(text).not.toMatch(/127\.0\.0\.1|:9\b|ECONNREFUSED/)
  })

  it("answers 504 when the upstream takes longer than the route allows", async () => {
    reply = (_req, res) => setTimeout(() => json(200, {})(_req, res), 500)
    const response = await call("readiness", {}, { timeoutMs: 50 })
    expect(response.status).toBe(504)
    expect(await response.json()).toEqual({ message: expect.any(String) })
  })

  it("never retries a write the upstream dropped", async () => {
    reply = (req) => req.socket.destroy()
    const response = await call("confirm", {
      method: "POST",
      headers: { Authorization: BEARER, "Content-Type": "application/json" },
      body: JSON.stringify({ decision: "approve" }),
    })
    expect(response.status).toBe(502)
    expect(seen).toHaveLength(1)
  })

  it("aborts the upstream request when the browser goes away", async () => {
    const closed = new Promise<void>((resolve) => {
      reply = (req) => req.socket.on("close", () => resolve())
    })
    const controller = new AbortController()
    const pending = forwardVoice(
      new Request("http://flow.test/api/voice/confirm", {
        method: "POST",
        body: "{}",
        signal: controller.signal,
      }),
      "confirm",
      { upstream }
    )
    await vi.waitFor(() => expect(seen).toHaveLength(1))
    controller.abort()
    await closed
    await pending
  })

  it.each([
    undefined,
    "",
    "http://office:3000/api",
    "http://user:pw@office:3000",
    "ftp://office:3000",
    "http://office:3000?x=1",
    "not a url",
  ])(
    "answers 502 for upstream setting %j without any request",
    async (value) => {
      const response = await forwardVoice(
        new Request("http://flow.test/api/voice/readiness"),
        "readiness",
        { upstream: value }
      )
      expect(response.status).toBe(502)
      expect(await response.json()).toEqual({ message: expect.any(String) })
      expect(seen).toHaveLength(0)
    }
  )

  it("logs only the route and failure kind: no token, body or file content", async () => {
    const lines: string[] = []
    for (const level of ["log", "info", "warn", "error", "debug"] as const)
      vi.spyOn(console, level).mockImplementation((...args) => {
        lines.push(args.map(String).join(" "))
      })
    await call(
      "tools",
      {
        method: "POST",
        headers: { Authorization: BEARER, "X-File-Name": "secret-file.pdf" },
        body: "secret-body-content",
      },
      { upstream: "http://127.0.0.1:9" }
    )
    expect(lines.length).toBeGreaterThan(0)
    const all = lines.join("\n")
    for (const secret of [
      "secret-token-123",
      "secret-body-content",
      "secret-file",
      "127.0.0.1",
    ])
      expect(all).not.toContain(secret)
  })
})
