// Mock surfaces from docs/design/web-redesign/canvas/glass.css that the shared components
// don't cover. Values are the mockup's; phones (< md) take the mockup's 760 px rules.

/** `.card`: glass, 24 px padding and 16 px gap (16/12 on phones), 20 px corners (18 on phones). */
export const card =
  "glass flex min-w-0 flex-col gap-3 rounded-[18px] p-4 md:gap-4 md:rounded-[20px] md:p-6"

/** `.tile`: a tinted block inside a card, never blurred. */
export const tile =
  "min-w-0 rounded-[16px] border border-border bg-(--glass-subtle) p-4"

/** `.tag`: small capitals over a title. */
export const tag = "text-[11px] font-extrabold tracking-[0.08em] uppercase"

/** `.h2`: section and page titles. */
export const h2 = "text-[22px] leading-[1.3] font-medium"

/** `.card-title`. */
export const cardTitle = "text-[15px] font-semibold tracking-[0.01em]"

/** `.chip-ic.gold`: an icon on a gold tint (gold text only on its own tint). */
export const goldChip =
  "inline-flex shrink-0 items-center justify-center bg-gold/22 text-(--tone-warning) dark:bg-gold/16"

/** `.ph-honey`: the warm photo placeholder. */
export const honey =
  "bg-[radial-gradient(120%_90%_at_30%_20%,#fbe7b4_0%,#e9bf6b_45%,#b98336_100%)]"
