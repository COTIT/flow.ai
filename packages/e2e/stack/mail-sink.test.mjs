import assert from "node:assert/strict"
import { spawn } from "node:child_process"
import { mkdtempSync, readdirSync, readFileSync } from "node:fs"
import { connect } from "node:net"
import { tmpdir } from "node:os"
import { join } from "node:path"
import process from "node:process"
import { test } from "node:test"

test("keeps each message as a file, dot-stuffing undone, through one session", async (t) => {
  const dir = mkdtempSync(join(tmpdir(), "mail-sink-"))
  const port = 20000 + Math.floor(Math.random() * 20000)
  const sink = spawn(process.execPath, [join(import.meta.dirname, "mail-sink.mjs")], {
    env: { ...process.env, MAIL_DIR: dir, MAIL_PORT: String(port) },
  })
  t.after(() => sink.kill())
  await new Promise((resolve) => sink.stdout.once("data", resolve))

  const socket = connect(port, "127.0.0.1")
  socket.setEncoding("utf8")
  let heard = ""
  socket.on("data", (d) => (heard += d))
  const said = (n) =>
    new Promise((resolve) => {
      const check = () => (heard.split("\r\n").length - 1 >= n ? resolve() : setTimeout(check, 10))
      check()
    })
  const message = (to) => `To: ${to}\r\nSubject: Hi\r\n\r\nLink: http://x/a?code=1\r\n..dotted\r\n.\r\n`
  await said(1)
  socket.write("EHLO e2e\r\nMAIL FROM:<a@e2e.test>\r\nRCPT TO:<b@e2e.test>\r\nDATA\r\n")
  await said(5)
  socket.write(message("b@e2e.test"))
  await said(6)
  socket.write("MAIL FROM:<a@e2e.test>\r\nRCPT TO:<c@e2e.test>\r\nDATA\r\n")
  await said(9)
  socket.write(`${message("c@e2e.test")}QUIT\r\n`)
  await said(11)
  socket.end()

  const files = readdirSync(dir).sort()
  assert.equal(files.length, 2)
  const first = readFileSync(join(dir, files[0]), "utf8")
  assert.match(first, /^To: b@e2e\.test/)
  assert.match(first, /code=1/)
  assert.match(first, /\r\n\.dotted$/)
  assert.match(readFileSync(join(dir, files[1]), "utf8"), /^To: c@e2e\.test/)
})
