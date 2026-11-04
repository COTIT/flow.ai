// The stack's mail catcher: a bare SMTP server (no TLS, no auth) that writes each message to
// MAIL_DIR as <time>-<n>.eml, so specs can read invite and verification links (STACK.md).
// Only ever on 127.0.0.1: nothing here may face a network.
import { mkdirSync, writeFileSync } from "node:fs"
import { createServer } from "node:net"
import { join } from "node:path"
import process from "node:process"

const dir = process.env.MAIL_DIR
const port = Number(process.env.MAIL_PORT)
mkdirSync(dir, { recursive: true })
let count = 0

createServer((socket) => {
  socket.setEncoding("utf8")
  const reply = (line) => socket.write(`${line}\r\n`)
  let buffer = ""
  let data = null // the message text while inside DATA, else null

  reply("220 e2e mail sink")
  socket.on("data", (chunk) => {
    buffer += chunk
    for (;;) {
      if (data !== null) {
        const end = buffer.indexOf("\r\n.\r\n")
        if (end < 0) return
        data += buffer.slice(0, end)
        buffer = buffer.slice(end + 5)
        // SMTP dot-stuffing: a line that starts with "." was sent as "..".
        writeFileSync(join(dir, `${Date.now()}-${++count}.eml`), data.replace(/^\.\./gm, "."))
        data = null
        reply("250 queued")
        continue
      }
      const nl = buffer.indexOf("\r\n")
      if (nl < 0) return
      const line = buffer.slice(0, nl)
      buffer = buffer.slice(nl + 2)
      const verb = line.slice(0, 4).toUpperCase()
      if (verb === "EHLO" || verb === "HELO") reply("250 e2e mail sink")
      else if (verb === "DATA") {
        data = ""
        reply("354 end with <CRLF>.<CRLF>")
      } else if (verb === "QUIT") {
        reply("221 bye")
        socket.end()
        return
      } else reply("250 ok") // MAIL, RCPT, RSET, NOOP
    }
  })
  socket.on("error", () => {})
}).listen(port, "127.0.0.1", () => console.log(`mail sink on 127.0.0.1:${port}, writing to ${dir}`))
