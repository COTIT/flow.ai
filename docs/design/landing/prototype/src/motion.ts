// Port of ../assets/mock.js + motion.js for the React prototype: theme toggle, decorative QR,
// reveal/assemble, the pinned journey and parallax. Counters are React Bits CountUp islands now.
export const reduceQuery = () => matchMedia("(prefers-reduced-motion: reduce)")

export function initMotion(root: HTMLElement): () => void {
  const html = document.documentElement
  const reduce = () => reduceQuery().matches
  html.classList.toggle("reduce", reduce())

  const toggles = [...root.querySelectorAll<HTMLElement>("[data-theme-toggle]")]
  const flip = () => html.classList.toggle("dark")
  toggles.forEach((b) => b.addEventListener("click", flip))

  root.querySelectorAll<HTMLElement>("[data-qr]").forEach((el) => {
    const n = 25, cell = 4
    let seed = 7, rects = ""
    const rnd = () => (seed = (seed * 9301 + 49297) % 233280) / 233280
    const finder = (x: number, y: number) => (x < 7 && y < 7) || (x >= n - 7 && y < 7) || (x < 7 && y >= n - 7)
    const ring = (x: number, y: number) => {
      const fx = x >= n - 7 ? x - (n - 7) : x, fy = y >= n - 7 ? y - (n - 7) : y
      return fx === 0 || fy === 0 || fx === 6 || fy === 6 || (fx >= 2 && fx <= 4 && fy >= 2 && fy <= 4)
    }
    for (let y = 0; y < n; y++)
      for (let x = 0; x < n; x++)
        if (finder(x, y) ? ring(x, y) : rnd() > 0.52) rects += `<rect x="${x * cell}" y="${y * cell}" width="${cell}" height="${cell}"/>`
    el.innerHTML = `<svg viewBox="0 0 ${n * cell} ${n * cell}" aria-hidden="true" fill="currentColor">${rects}</svg>`
  })
  root.querySelectorAll<HTMLElement>("[data-assemble]").forEach((el) =>
    [...el.children].forEach((c, i) => (c as HTMLElement).style.setProperty("--i", String(i)))
  )

  const smil = (el: Element, on: boolean) =>
    el.querySelectorAll("svg").forEach((s) => (on && !reduce() ? s.unpauseAnimations() : s.pauseAnimations()))
  const io = new IntersectionObserver(
    (entries) =>
      entries.forEach((e) => {
        e.target.classList.toggle("live", e.isIntersecting)
        smil(e.target, e.isIntersecting)
        if (e.isIntersecting) e.target.classList.add("in")
      }),
    { rootMargin: "0px 0px -8% 0px" }
  )
  root.querySelectorAll("[data-reveal],[data-assemble],[data-journey],[data-ambient]").forEach((el) => io.observe(el))

  const journeys = [...root.querySelectorAll<HTMLElement>("[data-journey]")].map((j) => {
    const route = j.querySelector<SVGPathElement>("[data-route]")
    const money = j.querySelector<SVGPathElement>("[data-money]")
    const coin = j.querySelector<SVGGElement>("[data-coin]")
    const rl = route?.getTotalLength() ?? 0
    if (route) route.style.strokeDasharray = String(rl)
    return { j, n: Number(j.dataset.journey), route, rl, money, ml: money?.getTotalLength() ?? 0, coin }
  })
  type J = (typeof journeys)[number]
  const paint = (o: J, p: number) => {
    const s = Math.min(o.n - 1, Math.floor(p * o.n))
    const lp = reduce() ? 1 : s === o.n - 1 ? Math.min(1, (p * o.n - s) * 1.6) : 0
    o.j.style.setProperty("--p", p.toFixed(4))
    o.j.style.setProperty("--lp", lp.toFixed(4))
    for (let i = 0; i < o.n; i++) o.j.classList.toggle("st" + i, i <= s)
    o.j.querySelectorAll<HTMLElement>("[data-step]").forEach((li) =>
      Number(li.dataset.step) === s ? li.setAttribute("aria-current", "step") : li.removeAttribute("aria-current")
    )
    if (o.route) o.route.style.strokeDashoffset = String(o.rl * (1 - Math.min(1, p * 1.15)))
    if (o.coin && o.money) {
      const pt = o.money.getPointAtLength(o.ml * lp)
      o.coin.setAttribute("transform", `translate(${pt.x} ${pt.y})`)
      o.coin.style.opacity = lp > 0 ? "1" : "0"
    }
  }

  const parallax = [...root.querySelectorAll<HTMLElement>("[data-parallax]")]
  let ticking = false
  const frame = () => {
    ticking = false
    const vh = innerHeight
    for (const o of journeys) {
      if (reduce()) { paint(o, 1); continue }
      const r = o.j.getBoundingClientRect()
      if (r.bottom < 0 || r.top > vh) continue
      paint(o, Math.min(1, Math.max(0, -r.top / Math.max(1, r.height - vh))))
    }
    for (const el of parallax) {
      if (reduce()) { el.style.transform = ""; continue }
      const r = el.getBoundingClientRect()
      if (r.bottom < -200 || r.top > vh + 200) continue
      el.style.transform = `translate3d(0, ${(scrollY * Number(el.dataset.parallax)).toFixed(1)}px, 0)`
    }
  }
  const onScroll = () => { if (!ticking) { ticking = true; requestAnimationFrame(frame) } }
  addEventListener("scroll", onScroll, { passive: true })
  addEventListener("resize", onScroll)
  frame()
  return () => {
    io.disconnect()
    toggles.forEach((b) => b.removeEventListener("click", flip))
    removeEventListener("scroll", onScroll)
    removeEventListener("resize", onScroll)
  }
}
