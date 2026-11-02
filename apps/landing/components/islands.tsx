"use client"

import {
  Component,
  lazy,
  Suspense,
  useSyncExternalStore,
  type ReactNode,
} from "react"

// React Bits pieces, each in its own chunk, fetched only when an island mounts.
const Particles = lazy(() => import("./rb/Particles"))
const BlurText = lazy(() => import("./rb/BlurText"))
const RotatingText = lazy(() => import("./rb/RotatingText"))
const LogoLoop = lazy(() => import("./rb/LogoLoop"))
const GlareHover = lazy(() => import("./rb/GlareHover"))
const CountUp = lazy(() => import("./rb/CountUp"))

const REDUCE = "(prefers-reduced-motion: reduce)"

/** False on the server and under reduced motion, so nothing animated is rendered or fetched there. */
function useMotionAllowed() {
  return useSyncExternalStore(
    (onChange) => {
      const query = matchMedia(REDUCE)
      query.addEventListener("change", onChange)
      return () => query.removeEventListener("change", onChange)
    },
    () => !matchMedia(REDUCE).matches,
    () => false
  )
}

/** If an animated piece fails (e.g. its chunk does not load), its spot keeps the plain version. */
class KeepPlain extends Component<
  { plain: ReactNode; children: ReactNode },
  { failed: boolean }
> {
  state = { failed: false }
  static getDerivedStateFromError() {
    return { failed: true }
  }
  render() {
    return this.state.failed ? this.props.plain : this.props.children
  }
}

/** The plain, server-rendered version until (and unless) the animated one is ready. */
function Island({
  plain,
  children,
}: {
  plain: ReactNode
  children: ReactNode
}) {
  if (!useMotionAllowed()) return plain
  return (
    <KeepPlain plain={plain}>
      <Suspense fallback={plain}>{children}</Suspense>
    </KeepPlain>
  )
}

const GOLD = ["#efc977", "#d0a55f", "#f9dd8c"]

/** Faint gold specks drifting behind the hero, like pollen. Decoration only. */
export function HeroPollen() {
  return (
    <Island plain={null}>
      <div className="rb-bg" aria-hidden="true">
        <Particles
          particleCount={140}
          particleSpread={10}
          speed={0.08}
          particleColors={GOLD}
          particleBaseSize={80}
          alphaParticles
          pixelRatio={1}
          disableRotation
        />
      </div>
    </Island>
  )
}

/** The headline comes into focus word by word; screen readers get the plain sentence. */
export function Headline({ text }: { text: string }) {
  return (
    <Island plain={text}>
      <span className="sr-only">{text}</span>
      <span aria-hidden="true">
        <BlurText text={text} animateBy="words" direction="top" delay={90} />
      </span>
    </Island>
  )
}

/** "Traced from the hive / harvest / …": the journey in one line. Plain: the hero tag. */
export function RotatingTag({
  plain,
  prefix,
  words,
}: {
  plain: string
  prefix: string
  words: string[]
}) {
  return (
    <Island plain={plain}>
      <span className="rotating">
        {prefix}
        <RotatingText
          texts={words}
          rotationInterval={2200}
          staggerDuration={0.02}
        />
      </span>
    </Island>
  )
}

/** A slow loop of what Genesy does. Plain: the same items as a still, wrapped list. */
export function FeatureLoop({
  label,
  items,
}: {
  label: string
  items: string[]
}) {
  const plain = (
    <ul className="loop-plain" aria-label={label}>
      {items.map((item) => (
        <li key={item} className="loop-chip">
          {item}
        </li>
      ))}
    </ul>
  )
  return (
    <Island plain={plain}>
      <LogoLoop
        logos={items.map((item) => ({
          node: <span className="loop-chip">{item}</span>,
          title: item,
        }))}
        speed={40}
        gap={16}
        logoHeight={40}
        fadeOut
        pauseOnHover
        ariaLabel={label}
      />
    </Island>
  )
}

/** A soft gold glare across a card on hover. */
export function Glare({ children }: { children: ReactNode }) {
  return (
    <Island plain={children}>
      <GlareHover
        glareColor="#f9dd8c"
        glareOpacity={0.3}
        background="transparent"
        borderColor="transparent"
        width="100%"
        height="100%"
        borderRadius="20px"
      >
        {children}
      </GlareHover>
    </Island>
  )
}

/** A number that ticks up when it comes into view. */
export function Count({ to, plain }: { to: number; plain: string }) {
  return (
    <Island plain={plain}>
      <CountUp to={to} separator="," duration={1.6} />
    </Island>
  )
}
