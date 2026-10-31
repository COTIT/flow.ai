import type { Surface } from '@/app/membership'
import { FARMER_CHAPTERS } from '@/app/tour/chapters/farmer'
import { OFFICER_CHAPTERS } from '@/app/tour/chapters/officer'
import { OPS_CHAPTERS } from '@/app/tour/chapters/ops'
import type { Chapter } from '@/app/tour/tourTypes'

export type { Chapter, TourStep } from '@/app/tour/tourTypes'

/**
 * One tour per surface, split into chapters — one per module — so a presenter
 * can play the module in front of them, or every chapter in order.
 *
 * Within a chapter each route gets one contiguous run of stops: a chapter that
 * visits Register, then People, then Register again has made the user travel
 * twice to say one thing. `tourSteps.test.ts` holds that, along with every
 * target existing and every string being a key.
 *
 * The copy carries the product's own rules, because this is the first thing a
 * new user reads and a tour is exactly where a sloppy summary would do damage:
 * an estimate is called an estimate, a price is indicative, an opportunity is
 * not a sale, capacity is planned and never measured.
 */
export const CHAPTERS: Record<Surface, Chapter[]> = {
  farmer: FARMER_CHAPTERS,
  officer: OFFICER_CHAPTERS,
  ops: OPS_CHAPTERS,
}
