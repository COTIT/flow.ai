import { headers } from "next/headers"

export async function ShopJsonLd({ data }: { data: object }) {
  const nonce = (await headers()).get("x-nonce") ?? undefined
  return (
    <script
      type="application/ld+json"
      nonce={nonce}
      dangerouslySetInnerHTML={{
        __html: JSON.stringify(data).replaceAll("<", "\\u003c"),
      }}
    />
  )
}
