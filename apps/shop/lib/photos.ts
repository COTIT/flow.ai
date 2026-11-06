/**
 * AFM's own photos (from africanfarmersmarketonline.com, with AFM's consent), resized WebP in
 * public/afm. Width and height are the files' own, so the browser reserves the space.
 * Alt text is the `photos.<key>` message.
 */
export const PHOTOS = {
  landscape: { src: "/afm/landscape.webp", width: 1200, height: 800 },
  illundo: { src: "/afm/illundo-beekeepers.webp", width: 640, height: 960 },
  farmers: { src: "/afm/farmers.webp", width: 960, height: 640 },
  mary: { src: "/afm/the-mary.webp", width: 560, height: 700 },
  benard: { src: "/afm/benard-omondi.webp", width: 800, height: 444 },
  nafasi: { src: "/afm/nafasi-masegesse.webp", width: 800, height: 484 },
} as const

export type PhotoKey = keyof typeof PHOTOS

/** A photo with its translated alt text, ready for <img> (or StoryCard's `photo`). */
export const photo = (key: PhotoKey, alt: string) => ({ ...PHOTOS[key], alt })
