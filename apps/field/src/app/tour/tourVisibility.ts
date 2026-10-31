/**
 * Whether an element is something a person can actually see in the viewport.
 *
 * DOM presence is not enough for a guided tour: a positioned tooltip can be
 * fully rendered thousands of pixels below the viewport while its overlay is
 * already swallowing every input. Walk the ancestor chain as well because
 * Joyride applies some of its transitional visibility to a wrapper around our
 * tooltip rather than to the tooltip itself.
 */
export function isVisibleInViewport(element: Element | null): element is Element {
  if (!element?.isConnected) return false

  let current: Element | null = element
  while (current) {
    const style = getComputedStyle(current)
    if (
      style.display === 'none' ||
      style.visibility === 'hidden' ||
      style.visibility === 'collapse' ||
      Number.parseFloat(style.opacity || '1') === 0
    ) {
      return false
    }
    current = current.parentElement
  }

  const rect = element.getBoundingClientRect()
  if (rect.width <= 0 || rect.height <= 0) return false

  const viewport = window.visualViewport
  const left = viewport?.offsetLeft ?? 0
  const top = viewport?.offsetTop ?? 0
  const width = viewport?.width || window.innerWidth || document.documentElement.clientWidth
  const height = viewport?.height || window.innerHeight || document.documentElement.clientHeight

  return rect.right > left && rect.left < left + width && rect.bottom > top && rect.top < top + height
}

/**
 * Put a tour target in the middle of the page's visible viewport.
 *
 * `scrollIntoView` is normally sufficient, but WebKit can decline to move the
 * document for a control inside a sticky container. Follow it with an explicit
 * document scroll calculated from the target's real screen position. Browsers
 * clamp the requested value at the page bounds, so this is also safe near the
 * top and bottom of a screen.
 */
export function scrollTourTargetIntoView(target: Element): void {
  target.scrollIntoView({ behavior: 'auto', block: 'center', inline: 'nearest' })

  const rect = target.getBoundingClientRect()
  const viewportHeight = window.visualViewport?.height || window.innerHeight
  const currentTop = window.scrollY || document.documentElement.scrollTop
  const nextTop = Math.max(0, Math.round(currentTop + rect.top - (viewportHeight - rect.height) / 2))

  window.scrollTo({ top: nextTop, behavior: 'auto' })
}
