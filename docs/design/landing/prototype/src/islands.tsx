import type { ReactNode } from "react"

import AnimatedContent from "./rb/AnimatedContent"
import BlurText from "./rb/BlurText"
import CountUp from "./rb/CountUp"
import DotGrid from "./rb/DotGrid"
import FadeContent from "./rb/FadeContent"
import GlareHover from "./rb/GlareHover"
import GradientText from "./rb/GradientText"
import LightRays from "./rb/LightRays"
import LogoLoop from "./rb/LogoLoop"
import Magnet from "./rb/Magnet"
import ClickSpark from "./rb/ClickSpark"
import Particles from "./rb/Particles"
import RotatingText from "./rb/RotatingText"
import ScrollFloat from "./rb/ScrollFloat"
import ScrollReveal from "./rb/ScrollReveal"
import ShinyText from "./rb/ShinyText"
import SpotlightCard from "./rb/SpotlightCard"
import SplitText from "./rb/SplitText"
import StarBorder from "./rb/StarBorder"
import TextType from "./rb/TextType"

// Brand gold (style guide 04.2025).
const GOLD = ["#956e46", "#f9dd8c", "#c3944b", "#efc977", "#956e46"]

export type Island = { text: string; html: string; props: Record<string, unknown>; dark: boolean }
const Html = ({ html }: { html: string }) => <div style={{ display: "contents" }} dangerouslySetInnerHTML={{ __html: html }} />

const FEATURES = ["Works offline in the field", "Kiswahili · English · Nederlands", "A QR passport per batch", "Web shop and till, one stock", "Farmer passbook", "Sharing only with consent", "Money exact to the cent"]

// Each island: which React Bits component fills a marked spot in the static page, and how.
export const ISLANDS: Record<string, (i: Island) => ReactNode> = {
  // A · Navy & Gold
  lightrays: () => <LightRays raysOrigin="top-center" raysColor="#efc977" raysSpeed={0.6} lightSpread={0.9} rayLength={1.4} maxDevicePixelRatio={1.5} />,
  shiny: ({ text }) => <ShinyText text={text} color="#d0a55f" shineColor="#f9dd8c" speed={3.5} />,
  split: ({ text }) => <SplitText text={text} tag="span" splitType="words" delay={60} duration={0.9} textAlign="left" />,
  star: () => (
    <StarBorder as="a" href="#contact" color="#efc977" backgroundColor="#efc977" textColor="#0a183a" duration={5}>
      {/* Mockup CSS is unlayered and wins over Tailwind's layer, so the label colour is set here. */}
      <span style={{ color: "#0a183a", fontWeight: 600 }}>Talk to us</span>
    </StarBorder>
  ),
  spotlight: ({ html, props }) => (
    <SpotlightCard spotlightColor="rgba(239, 201, 119, 0.22)" theme={(props.theme as "dark" | "light") ?? "light"} style={{ background: "transparent", border: 0, padding: 0 }}>
      <Html html={html} />
    </SpotlightCard>
  ),
  animated: ({ html }) => (
    <AnimatedContent distance={60} duration={1} scale={0.94} tilt={6}>
      <Html html={html} />
    </AnimatedContent>
  ),
  reveal: ({ text }) => <ScrollReveal baseOpacity={0.15} enableBlur blurStrength={4} textClassName="h2">{text}</ScrollReveal>,
  count: ({ props }) => <CountUp to={Number(props.to)} separator="," duration={1.6} />,

  // B · Mist & Journey
  particles: ({ dark }) => (
    <Particles particleCount={140} particleSpread={10} speed={0.08} particleColors={["#efc977", "#d0a55f", "#f9dd8c"]} particleBaseSize={dark ? 90 : 70} alphaParticles pixelRatio={1} disableRotation />
  ),
  blur: ({ text }) => <BlurText text={text} animateBy="words" direction="top" delay={90} />,
  rotating: () => (
    <span style={{ display: "inline-flex", gap: "0.4em", alignItems: "baseline" }}>
      Traced from the
      <RotatingText texts={["hive", "harvest", "batch", "passport", "shop", "passbook"]} rotationInterval={2200} staggerDuration={0.02} />
    </span>
  ),
  loop: () => (
    <LogoLoop
      logos={FEATURES.map((f) => ({ node: <span className="loop-chip">{f}</span>, title: f }))}
      speed={40}
      gap={16}
      logoHeight={40}
      fadeOut
      pauseOnHover
      ariaLabel="What Genesy does"
    />
  ),
  glare: ({ html }) => (
    <GlareHover glareColor="#f9dd8c" glareOpacity={0.35} background="transparent" borderColor="transparent" width="100%" height="100%" borderRadius="20px">
      <Html html={html} />
    </GlareHover>
  ),
  fade: ({ html }) => (
    <FadeContent blur duration={0.9}>
      <Html html={html} />
    </FadeContent>
  ),

  // C · Passport first
  dotgrid: ({ dark }) => (
    <DotGrid dotSize={4} gap={22} baseColor={dark ? "#1b2c57" : "#d1dce5"} activeColor="#d0a55f" proximity={120} dpr={1.5} />
  ),
  type: ({ text }) => <TextType text={text} typingSpeed={55} showCursor cursorCharacter="▍" loop={false} />,
  gradient: ({ text }) => <GradientText colors={GOLD} animationSpeed={6}>{text}</GradientText>,
  float: ({ text }) => <ScrollFloat textClassName="h2" stagger={0.02}>{text}</ScrollFloat>,
  magnet: ({ html }) => (
    <ClickSpark sparkColor={["#efc977", "#d0a55f"]} sparkCount={10}>
      <Magnet magnetStrength={4}>
        <Html html={html} />
      </Magnet>
    </ClickSpark>
  ),
}
