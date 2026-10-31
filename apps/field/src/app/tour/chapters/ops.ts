import { OPS_PROGRAMME_CHAPTERS } from '@/app/tour/chapters/opsProgramme'
import { OPS_SURVEY_CHAPTERS } from '@/app/tour/chapters/opsSurveys'
import { OPS_TOWER_CHAPTERS } from '@/app/tour/chapters/opsTower'
import type { Chapter } from '@/app/tour/tourTypes'

/**
 * The ops tour — shared by ops and admin — one chapter per module, in the order
 * of the sidebar. The Tower is a chapter here, not a role of its own.
 *
 * Written in three files so the chapters can be edited without touching one
 * another; the order here is the order of the menu and of "play all".
 */
export const OPS_CHAPTERS: Chapter[] = [
  ...OPS_PROGRAMME_CHAPTERS,
  ...OPS_SURVEY_CHAPTERS,
  ...OPS_TOWER_CHAPTERS,
]
