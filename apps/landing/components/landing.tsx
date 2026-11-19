import Link from "next/link"
import { getTranslations, setRequestLocale } from "next-intl/server"

import type { Locale } from "@verzo/i18n/locales"
import { Logo, LogoIcon } from "@verzo/ui/components/logo"
import { ThemeToggle } from "@verzo/ui/components/theme-toggle"
import {
  Count,
  FeatureLoop,
  Glare,
  Headline,
  HeroPollen,
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
              <p className="tag">{t("hero.tag")}</p>
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
            <ol className="scene platform-steps">
              {(t.raw("hero.steps") as string[]).map((step) => (
                <li key={step}>{step}</li>
              ))}
            </ol>
          </div>
        </div>
        <Journey />
        <div className="wrap loop-wrap">
          <FeatureLoop
            label={t("features.label")}
            items={t.raw("features.items") as string[]}
          />
        </div>
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
                      fill="#d9d3c8"
                    />
                    <path
                      d="M0 100 C 90 80 180 95 280 90 V120 H0Z"
                      fill="#5f5c55"
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
              <p style={{ fontSize: "14px", color: "#a8a69e" }}>
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
