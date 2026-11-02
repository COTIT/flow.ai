// Shared mockup behaviour: theme (?theme=light|dark or system), fake QR.
;(function () {
  const root = document.documentElement
  const param = new URLSearchParams(location.search).get("theme")
  const dark = param ? param === "dark" : matchMedia("(prefers-color-scheme: dark)").matches
  root.classList.toggle("dark", dark)

  document.addEventListener("DOMContentLoaded", () => {
    document.querySelectorAll("[data-theme-toggle]").forEach((b) =>
      b.addEventListener("click", () => root.classList.toggle("dark"))
    )

    // Decorative QR: finder squares + seeded noise. Not scannable on purpose.
    document.querySelectorAll("[data-qr]").forEach((el) => {
      const n = 25, cell = 4
      let seed = 7, rects = ""
      const rnd = () => ((seed = (seed * 9301 + 49297) % 233280) / 233280)
      const finder = (x, y) => x < 7 && y < 7 || x >= n - 7 && y < 7 || x < 7 && y >= n - 7
      const ring = (x, y) => {
        const fx = x >= n - 7 ? x - (n - 7) : x, fy = y >= n - 7 ? y - (n - 7) : y
        return fx === 0 || fy === 0 || fx === 6 || fy === 6 || (fx >= 2 && fx <= 4 && fy >= 2 && fy <= 4)
      }
      for (let y = 0; y < n; y++)
        for (let x = 0; x < n; x++)
          if (finder(x, y) ? ring(x, y) : rnd() > 0.52)
            rects += `<rect x="${x * cell}" y="${y * cell}" width="${cell}" height="${cell}"/>`
      el.innerHTML = `<svg viewBox="0 0 ${n * cell} ${n * cell}" aria-hidden="true" fill="currentColor">${rects}</svg>`
    })

  })
})()
