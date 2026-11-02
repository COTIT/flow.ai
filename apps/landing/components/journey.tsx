import { getTranslations } from "next-intl/server"

import { JourneyMotion } from "@/components/journey-motion"

/**
 * One jar's journey, hive to passbook, as an illustrated scene plus six written steps.
 * Rendered complete on the server; JourneyMotion pins it and plays it by scroll when motion is allowed.
 */
export async function Journey() {
  const t = await getTranslations("landing")
  return (
    <section
      className="journey"
      data-journey="7"
      id="journey"
      style={{ "--n": 7 } as React.CSSProperties}
      aria-labelledby="j-title"
    >
      <div className="j-pin">
        <div className="wrap j-grid">
          <div className="j-head">
            <span className="tag">{t("journey.tag")}</span>
            <h2 className="h2" id="j-title">
              {t("journey.title")}
            </h2>
            <ol className="j-steps">
              <li data-step="1">
                <b>{t("journey.steps.hive.title")}</b>
                <span className="muted">{t("journey.steps.hive.text")}</span>
              </li>
              <li data-step="2">
                <b>{t("journey.steps.harvest.title")}</b>
                <span className="muted">{t("journey.steps.harvest.text")}</span>
              </li>
              <li data-step="3">
                <b>{t("journey.steps.passport.title")}</b>
                <span className="muted">
                  {t("journey.steps.passport.text")}
                </span>
              </li>
              <li data-step="4">
                <b>{t("journey.steps.scan.title")}</b>
                <span className="muted">{t("journey.steps.scan.text")}</span>
              </li>
              <li data-step="5">
                <b>{t("journey.steps.sold.title")}</b>
                <span className="muted">{t("journey.steps.sold.text")}</span>
              </li>
              <li data-step="6">
                <b>{t("journey.steps.paid.title")}</b>
                <span className="muted">{t("journey.steps.paid.text")}</span>
              </li>
            </ol>
            <div className="j-progress" aria-hidden="true">
              <span />
            </div>
          </div>
          <div className="j-stage">
            <svg className="scene" viewBox="0 0 800 480" aria-hidden="true">
              <defs>
                <linearGradient id="bg" x1="0" x2="1">
                  <stop offset="0" stopColor="#956e46" />
                  <stop offset=".3" stopColor="#f9dd8c" />
                  <stop offset=".62" stopColor="#c3944b" />
                  <stop offset=".84" stopColor="#efc977" />
                  <stop offset="1" stopColor="#956e46" />
                </linearGradient>
              </defs>
              <rect
                className="sky"
                x="-100"
                y="-50"
                width="1000"
                height="600"
              />
              <g className="sun-wrap">
                <circle
                  className="sun"
                  cx="640"
                  cy="90"
                  r="44"
                  fill="url(#bg)"
                />
              </g>
              <path
                className="hill hl1"
                d="M-100 260 C 60 190 200 210 330 240 S 600 180 900 230 V600 H-100Z"
              />
              <g className="trees hl1">
                <rect x="452" y="196" width="5" height="34" rx="2" />
                <ellipse cx="455" cy="192" rx="30" ry="12" />
                <rect x="760" y="200" width="5" height="30" rx="2" />
                <ellipse cx="762" cy="196" rx="24" ry="10" />
              </g>
              <path
                className="hill hl2"
                d="M-100 330 C 80 280 260 300 420 320 S 700 280 900 310 V600 H-100Z"
              />
              <path
                className="hill hl3"
                d="M-100 420 C 120 390 360 410 520 400 S 760 380 900 400 V600 H-100Z"
              />
              <path
                className="road-faint"
                d="M100 285 C 170 205 200 235 240 235 S 330 295 390 275 S 480 215 540 225 S 640 295 690 275"
              />
              <path
                className="road"
                data-route
                d="M100 285 C 170 205 200 235 240 235 S 330 295 390 275 S 480 215 540 225 S 640 295 690 275"
              />
              <g className="j-part p6">
                <path
                  className="money-road"
                  d="M690 300 C 600 470 330 470 150 425"
                />
              </g>
              <path
                data-money
                d="M690 300 C 600 470 330 470 150 425"
                fill="none"
                stroke="none"
              />
              <g className="house">
                <path d="M48 420 l28-24 28 24v30h-56z" />
                <rect x="68" y="430" width="14" height="20" />
              </g>
              <g transform="translate(100 250) scale(0.9)">
                <g className="j-part p1">
                  <path d="M-38 34h76" />
                  <path
                    className="f"
                    d="M-32 34v-10h64v10zM-28 24v-12h56v12zM-22 12v-12h44v12zM-14 0v-10a14 14 0 0 1 28 0v10z"
                  />
                  <path className="door" d="M-6 34v-7a6 6 0 0 1 12 0v7z" />
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
              </g>
              <g transform="translate(240 196) scale(0.8)">
                <g className="j-part p2">
                  <rect
                    className="f"
                    x="-36"
                    y="-30"
                    width="72"
                    height="46"
                    rx="3"
                  />
                  <path d="M-36 -14h72M-36 0h72M-12 -30v46M12 -30v46" />
                  <path
                    className="hex"
                    d="M-26 -22l4-3 4 3v5l-4 3-4-3zM20 -8l4-3 4 3v5l-4 3-4-3z"
                  />
                  <path d="M0 16v10" />
                  <circle className="drop" cx="0" cy="32" r="4" />
                </g>
              </g>
              <g transform="translate(390 236) scale(0.9)">
                <g className="j-part p3">
                  <rect
                    className="lid"
                    x="-18"
                    y="-34"
                    width="36"
                    height="9"
                    rx="2"
                  />
                  <path
                    className="f honey"
                    d="M-24 -25h48v48a8 8 0 0 1-8 8h-32a8 8 0 0 1-8-8z"
                  />
                  <rect
                    className="label"
                    x="-15"
                    y="-8"
                    width="30"
                    height="20"
                    rx="2"
                  />
                  <g className="tag-swing">
                    <path d="M18 -26l20 8" />
                    <rect
                      className="tagcard"
                      x="32"
                      y="-20"
                      width="28"
                      height="34"
                      rx="3"
                    />
                    <path
                      className="qr"
                      d="M37 -14h7v7h-7zM48 -14h7v7h-7zM37 -3h7v7h-7zM49 -2h5v5h-5zM49 6h5v4h-5z"
                    />
                  </g>
                </g>
              </g>
              <g transform="translate(540 182) scale(0.8)">
                <g className="j-part p4">
                  <rect
                    className="f"
                    x="-22"
                    y="-40"
                    width="44"
                    height="80"
                    rx="8"
                  />
                  <rect
                    className="screen"
                    x="-16"
                    y="-30"
                    width="32"
                    height="52"
                    rx="2"
                  />
                  <path
                    className="qr"
                    d="M-10 -22h8v8h-8zM2 -22h8v8h-8zM-10 -10h8v8h-8zM3 -8h6v6h-6zM-2 4h12v4h-12z"
                  />
                  <path className="scan" d="M-16 -30h32" />
                  <circle cx="0" cy="31" r="3" />
                </g>
              </g>
              <g transform="translate(690 236) scale(0.9)">
                <g className="j-part p5">
                  <path className="awning" d="M-42 -22h84l-8-16h-68z" />
                  <path d="M-42 -22q7 10 14 0q7 10 14 0q7 10 14 0q7 10 14 0q7 10 14 0q7 10 14 0" />
                  <path className="f" d="M-36 -14v48h72v-48z" />
                  <path d="M-36 10h72" />
                  <path
                    className="honey"
                    d="M-26 -2h10v12h-10zM-6 -2h10v12h-10zM14 -2h10v12h-10z"
                  />
                </g>
              </g>
              <g transform="translate(160 400) scale(0.75)">
                <g className="j-part p6">
                  <path
                    className="f"
                    d="M-32 -34h46a16 16 0 0 1 16 16v52h-46a16 16 0 0 1-16-16z"
                  />
                  <path d="M-22 -20h30M-22 -10h30M-22 0h20" />
                  <path className="newline" d="M-22 12h32" />
                  <circle className="plus" cx="26" cy="26" r="11" />
                  <path className="plus-sign" d="M26 20v12M20 26h12" />
                </g>
              </g>
              <g data-coin transform="translate(150 425)">
                <circle r="11" className="coin" />
                <path d="M-4 -4h8M-4 0h8M-4 4h8" className="coin-mark" />
              </g>
            </svg>
          </div>
        </div>
      </div>
      <JourneyMotion />
    </section>
  )
}
