// Shared motion for the landing mockups. Plain JS: IntersectionObserver + one rAF scroll loop.
// Reduced motion: everything is painted in its final state, nothing pins, loops or ticks.
;(function () {
  const root = document.documentElement
  const mq = matchMedia("(prefers-reduced-motion: reduce)")
  const reduce = () => mq.matches
  root.classList.toggle("reduce", reduce())

  document.addEventListener("DOMContentLoaded", () => {
    const fmt = new Intl.NumberFormat("en")
    document.querySelectorAll("[data-assemble]").forEach((el) =>
      [...el.children].forEach((c, i) => c.style.setProperty("--i", i))
    )

    const count = (el) => {
      const to = Number(el.dataset.count)
      if (reduce()) return (el.textContent = fmt.format(to))
      const t0 = performance.now()
      const step = (t) => {
        const k = Math.min(1, (t - t0) / 1400)
        el.textContent = fmt.format(Math.round(to * (1 - (1 - k) ** 3)))
        if (k < 1) requestAnimationFrame(step)
      }
      requestAnimationFrame(step)
    }

    // SMIL loops (gold shine) run only while on screen and never under reduced motion.
    const smil = (el, on) =>
      el.querySelectorAll("svg").forEach((s) => (on && !reduce() ? s.unpauseAnimations?.() : s.pauseAnimations?.()))

    const io = new IntersectionObserver(
      (entries) =>
        entries.forEach((e) => {
          e.target.classList.toggle("live", e.isIntersecting)
          smil(e.target, e.isIntersecting)
          if (e.isIntersecting && !e.target.classList.contains("in")) {
            e.target.classList.add("in")
            if (e.target.dataset.count !== undefined) count(e.target)
          }
        }),
      { rootMargin: "0px 0px -8% 0px" }
    )
    document
      .querySelectorAll("[data-reveal],[data-count],[data-assemble],[data-journey],[data-ambient]")
      .forEach((el) => io.observe(el))

    // Journey: a tall section whose inner frame is pinned; scroll progress picks the stage.
    const journeys = [...document.querySelectorAll("[data-journey]")].map((j) => {
      const route = j.querySelector("[data-route]")
      const money = j.querySelector("[data-money]")
      const coin = j.querySelector("[data-coin]")
      const rl = route?.getTotalLength() ?? 0
      if (route) route.style.strokeDasharray = rl
      return { j, n: Number(j.dataset.journey), route, rl, money, ml: money?.getTotalLength() ?? 0, coin }
    })
    const paint = (o, p) => {
      const s = Math.min(o.n - 1, Math.floor(p * o.n))
      const lp = reduce() ? 1 : s === o.n - 1 ? Math.min(1, (p * o.n - s) * 1.6) : 0
      o.j.style.setProperty("--p", p.toFixed(4))
      o.j.style.setProperty("--lp", lp.toFixed(4))
      for (let i = 0; i < o.n; i++) o.j.classList.toggle("st" + i, i <= s)
      o.j.querySelectorAll("[data-step]").forEach((li) =>
        Number(li.dataset.step) === s ? li.setAttribute("aria-current", "step") : li.removeAttribute("aria-current")
      )
      if (o.route) o.route.style.strokeDashoffset = o.rl * (1 - Math.min(1, p * 1.15))
      if (o.coin && o.money) {
        const pt = o.money.getPointAtLength(o.ml * lp)
        o.coin.setAttribute("transform", `translate(${pt.x} ${pt.y})`)
        o.coin.style.opacity = lp > 0 ? 1 : 0
      }
    }

    const parallax = [...document.querySelectorAll("[data-parallax]")]
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
    mq.addEventListener("change", () => { root.classList.toggle("reduce", reduce()); onScroll() })
    frame()
  })
})()
