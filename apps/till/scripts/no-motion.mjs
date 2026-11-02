// The till never animates. Run after `next build`: fails when animation code reached the browser
// bundle (.next/static), or when the stylesheet lost the rule that switches every CSS transition and
// animation off. @keyframes from shared CSS may stay in the file: under that rule nothing can run them.
import { readdirSync, readFileSync } from "node:fs"
import { join, relative } from "node:path"
import process from "node:process"
import { fileURLToPath } from "node:url"

/** Animation code in JS: the `island` wrapper of @genesy/ui and the motion library. */
const MARKERS = ["data-motion", "framer-motion", "motion/react"]

/** app/till.css's last rule as the minifier writes it. */
export const KILL_RULE =
  ".till-app *,.till-app :before,.till-app :after{transition:none!important;animation:none!important}"

/** Problems found under a `.next/static` folder; empty when the bundle is still. */
export function findMotion(dir) {
  const files = readdirSync(dir, { recursive: true }).map(String)
  const problems = files
    .filter((f) => f.endsWith(".js"))
    .flatMap((f) => {
      const text = readFileSync(join(dir, f), "utf8")
      return MARKERS.filter((m) => text.includes(m)).map((m) => `${f}: ${m}`)
    })
  const killed = files
    .filter((f) => f.endsWith(".css"))
    .some((f) => readFileSync(join(dir, f), "utf8").includes(KILL_RULE))
  return killed
    ? problems
    : [...problems, "no stylesheet carries the till's no-animation rule"]
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const dir = fileURLToPath(new URL("../.next/static", import.meta.url))
  const problems = findMotion(dir)
  if (problems.length) {
    console.error(
      `Animation found in the till bundle (${relative(process.cwd(), dir)}):\n  ${problems.join("\n  ")}`
    )
    process.exit(1)
  }
  console.log("no-motion: the till bundle has no animation code")
}
