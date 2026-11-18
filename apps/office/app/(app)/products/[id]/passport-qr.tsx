"use client"

import * as React from "react"
import { createPortal, flushSync } from "react-dom"
import { PrinterIcon } from "lucide-react"
import { useTranslations } from "next-intl"
import { encode } from "uqr"

import { useStorefrontConnectionGet } from "@genesy/api-client/endpoints/storefront-connection"
import { Button } from "@genesy/ui/components/button"
import { Tile } from "@/components/bento"
import { storefrontProductPassportUrl } from "@/lib/storefront-url"

export function ProductPassportQr({
  workspaceId,
  productId,
  productName,
}: {
  workspaceId: string
  productId: string
  productName: string
}) {
  const connection = useStorefrontConnectionGet(workspaceId, {
    query: { enabled: Boolean(workspaceId), retry: false },
  })
  const url =
    !connection.isError && !connection.isFetching
      ? storefrontProductPassportUrl(
          process.env.NEXT_PUBLIC_SHOP_URL,
          connection.data?.data,
          productId
        )
      : null
  return url ? <PassportLabel name={productName} url={url} /> : null
}

function PassportLabel({ name, url }: { name: string; url: string }) {
  const t = useTranslations("products.passportQr")
  const [printing, setPrinting] = React.useState(false)
  React.useEffect(() => {
    const finish = () => setPrinting(false)
    window.addEventListener("afterprint", finish)
    return () => window.removeEventListener("afterprint", finish)
  }, [])
  const qr = React.useMemo(() => encode(url, { ecc: "M", border: 4 }), [url])
  const path = qr.data
    .flatMap((row, y) =>
      row.map((dark, x) => (dark ? `M${x} ${y}h1v1H${x}z` : ""))
    )
    .join("")
  const svg = (
    <svg
      role="img"
      aria-label={t("image", { name })}
      width={160}
      height={160}
      viewBox={`0 0 ${qr.size} ${qr.size}`}
      shapeRendering="crispEdges"
      className="mx-auto block max-w-full"
    >
      <rect width={qr.size} height={qr.size} fill="white" />
      <path d={path} fill="black" />
    </svg>
  )
  return (
    <Tile id="product-passport-qr" title={t("title")} span="">
      <div className="grid gap-3">
        {svg}
        <a
          href={url}
          target="_blank"
          rel="noreferrer"
          className="text-xs break-all underline underline-offset-2"
        >
          {url}
        </a>
        <Button
          type="button"
          variant="outline"
          className="w-fit"
          onClick={() => {
            flushSync(() => setPrinting(true))
            window.print()
          }}
        >
          <PrinterIcon aria-hidden />
          {t("print")}
        </Button>
      </div>
      {printing
        ? createPortal(
            <section className="product-passport-print">
              <h1>{name}</h1>
              {svg}
            </section>,
            document.body
          )
        : null}
    </Tile>
  )
}
