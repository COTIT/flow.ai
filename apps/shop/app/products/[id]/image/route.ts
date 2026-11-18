import { unstable_rethrow } from "next/navigation"

import { missingResponse, unavailableResponse } from "@/lib/crawl"
import { isUuid } from "@/lib/passport"
import { primaryImageSource } from "@/lib/product-image"
import { getShopBase } from "@/lib/shop-scope"
import { getProduct } from "@/lib/storefront"

const signatures: Record<string, number[]> = {
  "image/png": [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a],
  "image/jpeg": [0xff, 0xd8, 0xff],
  "image/gif": [0x47, 0x49, 0x46, 0x38],
  "image/webp": [0x52, 0x49, 0x46, 0x46],
}

async function matchesRaster(stream: ReadableStream<Uint8Array>, mime: string) {
  const reader = stream.getReader()
  const prefix = new Uint8Array(12)
  let size = 0
  try {
    while (size < prefix.length) {
      const { done, value } = await reader.read()
      if (done) break
      const chunk = value.subarray(0, prefix.length - size)
      prefix.set(chunk, size)
      size += chunk.length
    }
    const signature = signatures[mime]!
    if (
      size < signature.length ||
      !signature.every((byte, i) => byte === prefix[i])
    )
      return false
    if (mime === "image/gif")
      return (
        size >= 6 &&
        (prefix[4] === 0x37 || prefix[4] === 0x39) &&
        prefix[5] === 0x61
      )
    if (mime === "image/webp")
      return size >= 12 && String.fromCharCode(...prefix.subarray(8)) === "WEBP"
    return true
  } finally {
    // A tee branch's cancellation can wait for the other branch to drain.
    void reader.cancel().catch(() => undefined)
  }
}

export async function GET(
  _request: Request,
  { params }: { params: Promise<{ id: string }> }
) {
  const { id } = await params
  if (!isUuid(id) || (await getShopBase())) return missingResponse()

  try {
    const source = primaryImageSource(await getProduct(id))
    if (!source) return missingResponse()

    const response = await fetch(source.url, {
      cache: "no-store",
      credentials: "omit",
      redirect: "error",
      signal: AbortSignal.timeout(8000),
    })
    const mime = response.headers
      .get("content-type")
      ?.split(";")[0]
      ?.trim()
      .toLowerCase()
    if (response.status !== 200 || !response.body || mime !== source.mime) {
      void response.body?.cancel().catch(() => undefined)
      if (response.status === 200 || response.status === 404)
        return missingResponse()
      return unavailableResponse()
    }

    const [check, image] = response.body.tee()
    try {
      if (await matchesRaster(check, mime))
        return new Response(image, {
          headers: {
            "Cache-Control": "public, max-age=0, s-maxage=60",
            "Content-Type": mime,
            "X-Content-Type-Options": "nosniff",
          },
        })
    } catch (error) {
      void image.cancel().catch(() => undefined)
      throw error
    }
    void image.cancel().catch(() => undefined)
    return missingResponse()
  } catch (error) {
    unstable_rethrow(error)
    return unavailableResponse()
  }
}
