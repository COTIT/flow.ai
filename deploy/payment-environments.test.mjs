import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import test from "node:test"

// Keep the deployment fence testable without a Docker CLI or downloaded dependencies.
for (const [deployment, realm, command] of [
  ["staging", "sandbox", "seed"],
  ["production", "production", "cli"],
]) {
  test(`${deployment} pins API, workers and command payment realm`, () => {
    const source = readFileSync(
      new URL(
        `./${deployment}/docker-compose.${deployment}.yml`,
        import.meta.url
      ),
      "utf8"
    )
    const anchor = source.match(/^x-api-env: &api-env\n([\s\S]*?)(?=^\S)/m)?.[1]
    assert.ok(anchor, "shared API environment exists")
    assert.match(anchor, new RegExp(`^  GLOBADYME_ENVIRONMENT: ${realm}$`, "m"))
    assert.doesNotMatch(anchor, /GLOBADYME_ENVIRONMENT:.*\$/)
    for (const service of ["api", command]) {
      const block = source.match(
        new RegExp(
          `^  ${service}:\\n([\\s\\S]*?)(?=^  [a-z][a-z_-]*:|^\\S)`,
          "m"
        )
      )?.[1]
      assert.ok(block, `${service} exists`)
      assert.match(block, /^    environment: \*api-env$/m)
    }
  })
}
