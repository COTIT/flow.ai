import type { MetadataRoute } from "next"

import { webManifest } from "@genesy/ui/lib/pwa"

export default function manifest(): MetadataRoute.Manifest {
  return webManifest({
    name: "Genesy Till",
    shortName: "Till",
    // "/" only redirects here, and a redirect can't be served offline.
    startUrl: "/sale",
    themeColor: "#1a1a19",
    backgroundColor: "#faf8f3",
  })
}
