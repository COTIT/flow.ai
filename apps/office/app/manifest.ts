import type { MetadataRoute } from "next"

import { webManifest } from "@genesy/ui/lib/pwa"

export default function manifest(): MetadataRoute.Manifest {
  return webManifest({
    name: "Genesy Office",
    shortName: "Office",
    themeColor: "#1a1a19",
    backgroundColor: "#faf8f3",
  })
}
