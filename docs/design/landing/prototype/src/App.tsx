import { useEffect, useLayoutEffect, useRef, useState, type ReactPortal } from "react"
import { createPortal } from "react-dom"

import { ISLANDS } from "./islands"
import { initMotion, reduceQuery } from "./motion"
import aCss from "./pages/a.css?inline"
import aHtml from "./pages/a.html?raw"
import bCss from "./pages/b.css?inline"
import bHtml from "./pages/b.html?raw"
import cCss from "./pages/c.css?inline"
import cHtml from "./pages/c.html?raw"

const PAGES = {
  a: { name: "A · Navy & Gold", css: aCss, html: aHtml },
  b: { name: "B · Mist & Journey", css: bCss, html: bHtml },
  c: { name: "C · Passport first", css: cCss, html: cHtml },
} as const
type Dir = keyof typeof PAGES

const params = new URLSearchParams(location.search)
const dir: Dir = (params.get("d") as Dir) in PAGES ? (params.get("d") as Dir) : "a"
const theme = params.get("theme")
document.documentElement.classList.toggle("dark", theme ? theme === "dark" : matchMedia("(prefers-color-scheme: dark)").matches)

export default function App() {
  const page = PAGES[dir]
  const root = useRef<HTMLDivElement>(null)
  const [portals, setPortals] = useState<ReactPortal[]>([])

  // The static page is the calm, final version. With motion allowed, React Bits islands replace marked spots.
  useLayoutEffect(() => {
    const el = root.current!
    el.innerHTML = page.html
    if (!reduceQuery().matches) {
      const dark = document.documentElement.classList.contains("dark")
      setPortals(
        [...el.querySelectorAll<HTMLElement>("[data-rb]")].map((host, i) => {
          const island = { text: host.textContent?.trim() ?? "", html: host.innerHTML, props: JSON.parse(host.dataset.props ?? "{}"), dark }
          host.innerHTML = ""
          return createPortal(ISLANDS[host.dataset.rb!](island), host, i)
        })
      )
    }
  }, [page])
  useEffect(() => initMotion(root.current!), [page])

  return (
    <>
      <style>{page.css}</style>
      <div ref={root} />
      {portals}
      <nav className="design-bar" aria-label="Design prototype switcher">
        {(Object.keys(PAGES) as Dir[]).map((d) => (
          <a key={d} href={`?d=${d}${theme ? `&theme=${theme}` : ""}`} aria-current={d === dir ? "page" : undefined}>{d.toUpperCase()}</a>
        ))}
        <a href={`?d=${dir}&theme=light`}>Light</a>
        <a href={`?d=${dir}&theme=dark`}>Dark</a>
      </nav>
    </>
  )
}
