import { readFile } from "node:fs/promises"
import { ImageResponse } from "next/og"
import { getTranslations } from "next-intl/server"

import type { Locale } from "@verzo/i18n/locales"

export const ogSize = { width: 1200, height: 630 }

// Literal paths let Next trace both files for the standalone build.
const FONT = new URL(
  "../../../packages/ui/src/fonts/Poppins-Medium.ttf",
  import.meta.url
)
const LOCKUP = new URL(
  "../../../docs/design/verzo/assets/lockup-dark.svg",
  import.meta.url
)

export async function ogImage(locale: Locale) {
  const [t, font, lockup] = await Promise.all([
    getTranslations({ locale, namespace: "landing" }),
    readFile(FONT),
    readFile(LOCKUP),
  ])
  return new ImageResponse(
    <div
      style={{
        width: "100%",
        height: "100%",
        display: "flex",
        flexDirection: "column",
        justifyContent: "center",
        gap: 38,
        padding: "0 96px",
        background: "#0a183a",
        color: "#faf8f3",
        fontFamily: "Poppins",
      }}
    >
      <img
        src={`data:image/svg+xml;base64,${lockup.toString("base64")}`}
        width={274}
        height={64}
        alt=""
      />
      <div style={{ fontSize: 76, lineHeight: 1.08 }}>{t("hero.title")}</div>
      <div style={{ fontSize: 30, color: "#efc977" }}>{t("hero.tag")}</div>
    </div>,
    {
      ...ogSize,
      fonts: [{ name: "Poppins", data: font, weight: 500, style: "normal" }],
    }
  )
}
