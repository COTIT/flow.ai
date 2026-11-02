import Link from "next/link"
import { getTranslations, setRequestLocale } from "next-intl/server"

import type { Locale } from "@genesy/i18n/locales"
import { Logo, LogoIcon } from "@genesy/ui/components/logo"
import { ThemeToggle } from "@genesy/ui/components/theme-toggle"
import {
  Count,
  FeatureLoop,
  Glare,
  Headline,
  HeroPollen,
  RotatingTag,
} from "@/components/islands"
import { Journey } from "@/components/journey"
import { LanguageLinks } from "@/components/language-links"
import { appLinks, contactHref } from "@/lib/site"

import "./landing.css"

const HOME: Record<Locale, string> = { en: "/", nl: "/nl", sw: "/sw" }

/**
 * The whole landing page for one language, from the approved direction-B mockup
 * (docs/design/landing/b-mist-journey). Server-rendered; motion is added on top.
 */
export async function Landing({ locale }: { locale: Locale }) {
  setRequestLocale(locale)
  const t = await getTranslations("landing")
  const home = HOME[locale]
  // Read at request time through an alias: Next inlines `process.env.NEXT_PUBLIC_*` at build time,
  // and this page renders per request (CSP nonce), so compose can set them without a rebuild.
  const env = process.env
  const links = appLinks(env)
  const contact = contactHref(env.NEXT_PUBLIC_CONTACT_EMAIL)
  const email = contact.slice("mailto:".length, contact.indexOf("?"))
  return (
    <div className="landing">
      <a className="sr-only" href="#main">
        {t("skip")}
      </a>
      <header>
        <div className="wrap bar">
          <Link href={home} aria-label={t("nav.home")}>
            <Logo className="logo" />
          </Link>
          <nav aria-label={t("nav.label")}>
            <a href="#journey">{t("nav.how")}</a>
            <a href="#who">{t("nav.who")}</a>
            <a href="#passport">{t("nav.passport")}</a>
            <a href="#contact">{t("nav.contact")}</a>
          </nav>
          <div className="tools">
            <LanguageLinks locale={locale} short />
            <ThemeToggle label={t("nav.theme")} />
            <a className="btn btn-primary" href="#contact">
              {t("cta")}
            </a>
          </div>
        </div>
      </header>
      <main id="main">
        <div className="hero">
          <HeroPollen />
          <div className="wrap hero-grid">
            <div className="copy">
              <span className="tag">
                <RotatingTag
                  plain={t("hero.tag")}
                  prefix={t("hero.rotating.prefix")}
                  words={t.raw("hero.rotating.words") as string[]}
                />
              </span>
              <h1 className="h1">
                <Headline text={t("hero.title")} />
              </h1>
              <p className="lead muted">{t("hero.lead")}</p>
              <div className="actions">
                <a className="btn btn-primary gold-shine" href="#contact">
                  {t("cta")}
                </a>
                <a className="btn btn-ghost" href="#journey">
                  {t("hero.how")}
                </a>
              </div>
            </div>
            <svg
              className="scene hero-scene"
              data-ambient
              viewBox="0 0 560 380"
              role="img"
              aria-label={t("hero.scene")}
            >
              <defs>
                <linearGradient id="g" x1="0" x2="1">
                  <stop offset="0" stopColor="#956e46" />
                  <stop offset=".3" stopColor="#f9dd8c" />
                  <stop offset=".62" stopColor="#c3944b" />
                  <stop offset=".84" stopColor="#efc977" />
                  <stop offset="1" stopColor="#956e46" />
                </linearGradient>
              </defs>
              <circle
                className="sun-pulse"
                cx="420"
                cy="110"
                r="48"
                fill="url(#g)"
                opacity=".9"
              />
              <path
                d="M0 250 C 90 190 170 200 260 235 S 430 190 560 220 V380 H0Z"
                fill="var(--hill-1)"
              />
              <path
                d="M0 300 C 120 250 220 280 320 290 S 470 250 560 280 V380 H0Z"
                fill="var(--hill-2)"
              />
              <g fill="var(--hill-3)">
                <rect x="104" y="196" width="6" height="40" rx="2" />
                <ellipse cx="107" cy="190" rx="34" ry="14" />
                <rect x="186" y="210" width="5" height="30" rx="2" />
                <ellipse cx="188" cy="205" rx="24" ry="10" />
              </g>
              <g transform="translate(70 250)">
                <rect width="66" height="14" rx="2" fill="var(--hill-3)" />
                <rect x="4" y="14" width="58" height="40" fill="#d0a55f" />
                <rect x="4" y="30" width="58" height="3" fill="#956e46" />
                <rect
                  x="26"
                  y="44"
                  width="14"
                  height="5"
                  rx="2"
                  fill="#0a183a"
                />
              </g>
              <g fill="#0a183a" transform="translate(100 230)">
                <g className="bees">
                  <g className="bee">
                    <ellipse
                      className="bee-body"
                      cx="30"
                      cy="-40"
                      rx="5.5"
                      ry="4"
                    />
                    <path className="wing" d="M28 -44q2-8 7-2" />
                  </g>
                  <g className="bee">
                    <ellipse
                      className="bee-body"
                      cx="44"
                      cy="-20"
                      rx="5.5"
                      ry="4"
                    />
                    <path className="wing" d="M42 -24q2-8 7-2" />
                  </g>
                  <g className="bee">
                    <ellipse
                      className="bee-body"
                      cx="-36"
                      cy="-30"
                      rx="5.5"
                      ry="4"
                    />
                    <path className="wing" d="M-38 -34q2-8 7-2" />
                  </g>
                </g>
              </g>
              <path
                d="M140 290 C 220 340 300 250 380 300 S 450 330 470 300"
                fill="none"
                stroke="var(--accent)"
                strokeWidth="3"
                strokeDasharray="2 10"
                strokeLinecap="round"
              />
              <g transform="translate(450 214)">
                <rect
                  x="6"
                  y="0"
                  width="44"
                  height="12"
                  rx="3"
                  fill="#0a183a"
                />
                <path
                  d="M2 14h52v58a10 10 0 0 1-10 10H12A10 10 0 0 1 2 72Z"
                  fill="url(#g)"
                />
                <rect x="12" y="36" width="32" height="24" rx="3" fill="#fff" />
                <rect x="17" y="41" width="10" height="10" fill="#0a183a" />
                <rect x="30" y="42" width="10" height="3" fill="#456087" />
                <rect x="30" y="48" width="8" height="3" fill="#456087" />
              </g>
            </svg>
          </div>
        </div>
        <div className="wrap loop-wrap">
          <FeatureLoop
            label={t("features.label")}
            items={t.raw("features.items") as string[]}
          />
        </div>
        <Journey />
        <section id="who" style={{ paddingTop: "0" }}>
          <div className="wrap">
            <div className="head" data-reveal>
              <span className="tag">{t("who.tag")}</span>
              <h2 className="h2">{t("who.title")}</h2>
            </div>
            <div className="cards">
              <article data-reveal className="card featured">
                <Glare>
                  <span className="icon">
                    <svg
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="1.8"
                    >
                      <path d="M12 22c4-4 7-7.5 7-11a7 7 0 0 0-14 0c0 3.5 3 7 7 11z" />
                      <circle cx="12" cy="11" r="2.5" />
                    </svg>
                  </span>
                  <h3 className="h3">{t("who.programmes.title")}</h3>
                  <p className="muted">{t("who.programmes.text")}</p>
                  <ul>
                    <li>{t("who.programmes.p1")}</li>
                    <li>{t("who.programmes.p2")}</li>
                    <li>{t("who.programmes.p3")}</li>
                  </ul>
                  <a href="#contact">{t("who.programmes.link")}</a>
                </Glare>
              </article>
              <article data-reveal className="card">
                <Glare>
                  <span className="icon">
                    <svg
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="1.8"
                    >
                      <path d="M4 9l1-5h14l1 5M4 9v11h16V9M4 9h16M9 20v-6h6v6" />
                    </svg>
                  </span>
                  <h3 className="h3">{t("who.brands.title")}</h3>
                  <p className="muted">{t("who.brands.text")}</p>
                  <ul>
                    <li>{t("who.brands.p1")}</li>
                    <li>{t("who.brands.p2")}</li>
                    <li>{t("who.brands.p3")}</li>
                  </ul>
                  <a href="#contact">{t("who.brands.link")}</a>
                </Glare>
              </article>
              <article data-reveal className="card">
                <Glare>
                  <span className="icon">
                    <svg
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="1.8"
                    >
                      <rect x="6" y="2" width="12" height="20" rx="2" />
                      <path d="M10 18h4" />
                    </svg>
                  </span>
                  <h3 className="h3">{t("who.buyers.title")}</h3>
                  <p className="muted">{t("who.buyers.text")}</p>
                  <ul>
                    <li>{t("who.buyers.p1")}</li>
                    <li>{t("who.buyers.p2")}</li>
                    <li>{t("who.buyers.p3")}</li>
                  </ul>
                  <a href={links.shop}>{t("who.buyers.link")}</a>
                </Glare>
              </article>
            </div>
          </div>
        </section>
        <section id="passport" style={{ paddingTop: "0" }}>
          <div className="wrap pp">
            <div style={{ display: "grid", gap: "20px" }}>
              <span className="tag">{t("passport.tag")}</span>
              <h2 className="h2">{t("passport.title")}</h2>
              <p className="lead muted">{t("passport.lead")}</p>
              <ul className="checks">
                <li>
                  <svg
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="2"
                  >
                    <circle cx="12" cy="12" r="10" />
                    <path d="m8 12 3 3 5-6" />
                  </svg>
                  <div>
                    <b>{t("passport.c1.title")}</b>
                    <br />
                    <span className="muted">{t("passport.c1.text")}</span>
                  </div>
                </li>
                <li>
                  <svg
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="2"
                  >
                    <circle cx="12" cy="12" r="10" />
                    <path d="m8 12 3 3 5-6" />
                  </svg>
                  <div>
                    <b>{t("passport.c2.title")}</b>
                    <br />
                    <span className="muted">{t("passport.c2.text")}</span>
                  </div>
                </li>
                <li>
                  <svg
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="2"
                  >
                    <circle cx="12" cy="12" r="10" />
                    <path d="m8 12 3 3 5-6" />
                  </svg>
                  <div>
                    <b>{t("passport.c3.title")}</b>
                    <br />
                    <span className="muted">{t("passport.c3.text")}</span>
                  </div>
                </li>
              </ul>
            </div>
            <div data-reveal className="phone" aria-label={t("passport.phone")}>
              <div className="screen">
                <div className="band">
                  <Logo tone="onDark" />
                </div>
                <div className="pic">
                  <svg viewBox="0 0 280 120" width="100%" aria-hidden="true">
                    <path
                      d="M0 80 C 70 50 140 60 200 75 S 260 60 280 70 V120 H0Z"
                      fill="#b9c9d6"
                    />
                    <path
                      d="M0 100 C 90 80 180 95 280 90 V120 H0Z"
                      fill="#456087"
                    />
                    <circle cx="220" cy="38" r="18" fill="#efc977" />
                  </svg>
                </div>
                <div className="inner" data-assemble>
                  <div
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      alignItems: "center",
                    }}
                  >
                    <b>{t("passport.product")}</b>
                    <span className="badge">{t("passport.example")}</span>
                  </div>
                  <div className="kv">
                    <span>{t("passport.batch")}</span>
                    <span>GN-0000-EX</span>
                  </div>
                  <div className="kv">
                    <span>{t("passport.origin")}</span>
                    <span>{t("passport.originValue")}</span>
                  </div>
                  <div className="kv">
                    <span>{t("passport.harvested")}</span>
                    <span>{t("passport.harvestedValue")}</span>
                  </div>
                  <div className="kv">
                    <span>{t("passport.beekeepers")}</span>
                    <Count to={38} plain="38" />
                  </div>
                  <div className="kv">
                    <span>{t("passport.jars")}</span>
                    <Count to={1240} plain="1,240" />
                  </div>
                  <div className="ok">
                    <svg
                      viewBox="0 0 24 24"
                      width="18"
                      fill="none"
                      stroke="#01909b"
                      strokeWidth="2.4"
                    >
                      <path d="m5 12 4 4 10-10" />
                    </svg>
                    {t("passport.paid")}
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>
        <section style={{ paddingTop: "0" }}>
          <div className="wrap">
            <div className="head" data-reveal>
              <span className="tag">{t("promise.tag")}</span>
              <h2 className="h2">{t("promise.title")}</h2>
            </div>
            <div className="promise" data-assemble>
              <div>
                <h3 className="h3">{t("promise.private.title")}</h3>
                <p className="muted">{t("promise.private.text")}</p>
              </div>
              <div>
                <h3 className="h3">{t("promise.consent.title")}</h3>
                <p className="muted">{t("promise.consent.text")}</p>
              </div>
              <div>
                <h3 className="h3">{t("promise.exact.title")}</h3>
                <p className="muted">{t("promise.exact.text")}</p>
              </div>
            </div>
          </div>
        </section>
        <section id="contact" style={{ paddingTop: "0" }}>
          <div className="wrap">
            <div className="cta">
              <LogoIcon className="icon-bg" aria-hidden="true" />
              <span className="tag" style={{ color: "var(--gold-hi)" }}>
                {t("contact.tag")}
              </span>
              <h2 className="h2" style={{ maxWidth: "640px" }}>
                {t("contact.title")}
              </h2>
              <a className="mail" href={contact}>
                {email}
              </a>
              <a className="btn btn-primary" href={contact}>
                {t("contact.button")}
              </a>
              <p style={{ fontSize: "14px", color: "#b4c3d3" }}>
                {t("contact.note")}
              </p>
            </div>
          </div>
        </section>
      </main>
      <footer>
        <div className="wrap">
          <div className="f-grid">
            <div
              style={{ display: "grid", gap: "14px", alignContent: "start" }}
            >
              <Logo className="logo" />
              <p className="muted" style={{ maxWidth: "300px" }}>
                {t("footer.tagline")}
              </p>
            </div>
            <div>
              <h3 className="h3">{t("footer.shop")}</h3>
              <ul>
                <li>
                  <a href={links.shop}>{t("footer.buy")}</a>
                </li>
                <li>
                  <a href={links.shop}>{t("footer.check")}</a>
                </li>
              </ul>
            </div>
            <div>
              <h3 className="h3">{t("footer.signIn")}</h3>
              <ul>
                <li>
                  <a href={links.office}>{t("footer.office")}</a>
                </li>
                <li>
                  <a href={links.field}>{t("footer.field")}</a>
                </li>
                <li>
                  <a href={links.till}>{t("footer.till")}</a>
                </li>
              </ul>
            </div>
            <div>
              <h3 className="h3">{t("footer.about")}</h3>
              <ul>
                <li>
                  <a href="#contact">{t("footer.contact")}</a>
                </li>
              </ul>
            </div>
          </div>
          <div className="legal">
            <span>
              {t("footer.copyright", { year: new Date().getFullYear() })}
            </span>
            <span>
              <LanguageLinks locale={locale} />
            </span>
          </div>
        </div>
      </footer>
    </div>
  )
}
