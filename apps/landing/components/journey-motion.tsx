"use client"

import { useEffect } from "react"

/**
 * Plays the journey by scroll (port of the approved mockup's engine): pins the section, picks a
 * stage from scroll progress, draws the road and rolls the coin home; also fades sections in.
 * With reduced motion it does nothing, so the page keeps its still, complete server state.
 */
export function JourneyMotion() {
  useEffect(() => {
    if (matchMedia("(prefers-reduced-motion: reduce)").matches) return
    const root = document.querySelector<HTMLElement>(".landing")
    const j = document.querySelector<HTMLElement>("[data-journey]")
    if (!root || !j) return

    root
      .querySelectorAll<HTMLElement>("[data-assemble]")
      .forEach((el) =>
        [...el.children].forEach((c, i) =>
          (c as HTMLElement).style.setProperty("--i", String(i))
        )
      )
    root.classList.add("motion")

    const io = new IntersectionObserver(
      (entries) =>
        entries.forEach((e) => {
          e.target.classList.toggle("live", e.isIntersecting)
          if (e.isIntersecting) e.target.classList.add("in")
        }),
      { rootMargin: "0px 0px -8% 0px" }
    )
    root
      .querySelectorAll(
        "[data-reveal],[data-assemble],[data-journey],[data-ambient]"
      )
      .forEach((el) => io.observe(el))

    const n = Number(j.dataset.journey)
    const route = j.querySelector<SVGPathElement>("[data-route]")
    const money = j.querySelector<SVGPathElement>("[data-money]")
    const coin = j.querySelector<SVGGElement>("[data-coin]")
    const steps = [...j.querySelectorAll<HTMLElement>("[data-step]")]
    const rl = route?.getTotalLength() ?? 0
    const ml = money?.getTotalLength() ?? 0
    if (route) route.style.strokeDasharray = String(rl)

    const paint = (p: number) => {
      const s = Math.min(n - 1, Math.floor(p * n))
      // The last stage rolls the coin home along the money road.
      const lp = s === n - 1 ? Math.min(1, (p * n - s) * 1.6) : 0
      j.style.setProperty("--p", p.toFixed(4))
      j.style.setProperty("--lp", lp.toFixed(4))
      for (let i = 0; i < n; i++) j.classList.toggle(`st${i}`, i <= s)
      steps.forEach((li) =>
        Number(li.dataset.step) === s
          ? li.setAttribute("aria-current", "step")
          : li.removeAttribute("aria-current")
      )
      if (route)
        route.style.strokeDashoffset = String(rl * (1 - Math.min(1, p * 1.15)))
      if (coin && money) {
        const pt = money.getPointAtLength(ml * lp)
        coin.setAttribute("transform", `translate(${pt.x} ${pt.y})`)
        coin.style.opacity = lp > 0 ? "1" : "0"
      }
    }

    let ticking = false
    const frame = () => {
      ticking = false
      const r = j.getBoundingClientRect()
      if (r.bottom < 0 || r.top > innerHeight) return
      paint(
        Math.min(1, Math.max(0, -r.top / Math.max(1, r.height - innerHeight)))
      )
    }
    const onScroll = () => {
      if (!ticking) {
        ticking = true
        requestAnimationFrame(frame)
      }
    }
    addEventListener("scroll", onScroll, { passive: true })
    addEventListener("resize", onScroll)
    paint(0)
    frame()
    return () => {
      io.disconnect()
      removeEventListener("scroll", onScroll)
      removeEventListener("resize", onScroll)
      root.classList.remove("motion")
    }
  }, [])
  return null
}
