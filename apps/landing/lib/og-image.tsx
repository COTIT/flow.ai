import { readFile } from "node:fs/promises"
import { ImageResponse } from "next/og"
import { getTranslations } from "next-intl/server"

import type { Locale } from "@genesy/i18n/locales"

export const ogSize = { width: 1200, height: 630 }

// Literal URLs, so the bundler ships exactly these two files with the server (Poppins from packages/ui).
const FONT = new URL(
  "../../../packages/ui/src/fonts/Poppins-Medium.ttf",
  import.meta.url
)
const ICON = new URL("../app/icon.svg", import.meta.url)

/** The share card: navy ground, the gold brand bars, the promise in Poppins. */
export async function ogImage(locale: Locale) {
  const t = await getTranslations({ locale, namespace: "landing" })
  const [font, icon] = await Promise.all([readFile(FONT), readFile(ICON)])
  return new ImageResponse(
    <div
      style={{
        width: "100%",
        height: "100%",
        display: "flex",
        alignItems: "center",
        gap: 64,
        padding: "0 96px",
        background: "#0a183a",
        color: "#f2f6f7",
        fontFamily: "Poppins",
      }}
    >
      <img
        src={`data:image/svg+xml;base64,${icon.toString("base64")}`}
        width={260}
        height={196}
        alt=""
      />
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 24,
          width: 684,
        }}
      >
        <div style={{ fontSize: 30, letterSpacing: 6, color: "#efc977" }}>
          GENESY
        </div>
        <div
          style={{
            fontSize: 52,
            lineHeight: 1.15,
            textTransform: "uppercase",
            letterSpacing: 1,
          }}
        >
          {t("hero.title")}
        </div>
      </div>
    </div>,
    {
      ...ogSize,
      fonts: [{ name: "Poppins", data: font, weight: 500, style: "normal" }],
    }
  )
}
