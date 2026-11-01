# Swahili handover — Ruaha 360

Generated 2026-10-19 from `src/i18n/en/common.json`. Do not edit by hand;
regenerate with `pnpm i18n:handover`.

## What this is

Every user-facing string in the application, with its Swahili **draft**,
for a **native Kiswahili reviewer** to correct. The string list is frozen:
every screen is built, so the keys will not move while the work is under
way.

**The Swahili here is an unreviewed draft.** It was written by Claude from
a sourced glossary (`docs/i18n-glossary.md`) and back-translated blind as a
cross-check. That catches some errors and not others, and a native reader
has not seen it. It ships so the demo is usable in Swahili, with the
status column recording honestly which strings a native reviewer has been
through. Where a draft is wrong, correct it: a wrong Swahili string is
worse than English, because English is visibly untranslated and a wrong
string is not.

## Priority

- **Required** — the farmer and officer surfaces, and the chrome both of
  them render. 753 strings. CLAUDE.md specifies these ship
  complete Swahili.
- **Optional** — Ops and Tower. 546 strings. These may ship
  English for the demo.

1299 strings in total: 2 reviewed, 1297 draft, 0 missing. 357 flagged for a closer look (see the Flag note).

## How to read the table

- **Status** is `draft` until a native reviewer has been through the
  string, `reviewed` once they have, and `missing` if there is no Swahili
  yet.
- `{{value}}` and its siblings are placeholders. They must appear in the
  translation exactly as written, or the string will render broken.
- The Notes column carries product requirements from CLAUDE.md, not style
  advice. "Estimate", "indicative" and "planned capacity" are claims about
  what the programme does and does not promise; a translation that
  strengthens them misrepresents it.
- A survey incentive is a fixed cash amount paid at the office. It is never
  earnings, a wallet, a balance or a payment.
- Terms marked `unverified` in `docs/i18n-glossary.md` had no Tanzanian
  source. Check those first.
- Flag anything whose meaning is unclear rather than guessing. A gap is a
  question; a wrong string is a defect nobody sees.

## Required — farmer and officer surfaces

| Key | English | Swahili | Status | Notes |
| --- | --- | --- | --- | --- |
| `a11y.breadcrumb` | Breadcrumb | Njia ya kurasa | draft | Flag: Drafter: 'Njia ya kurasa' composed; no glossary term for breadcrumb (screen-reader label only). |
| `a11y.language` | Language | Lugha | draft | Flag: Drafter: 'Lugha' unverified as UI label. |
| `a11y.primaryNav` | Primary navigation | Urambazaji mkuu | draft |  |
| `a11y.skipToContent` | Skip to content | Ruka hadi kwenye maudhui | draft | Flag: Drafter: 'Ruka hadi kwenye maudhui' composed; 'Ruka' unverified. |
| `auditTrail.blocked` | Could not redeem: {{reason}} | Haikuweza kukabidhiwa: {{reason}} | draft | Keep {{reason}} Flag: Drafter: 'Could not redeem' -> 'Haikuweza kukabidhiwa' (redeem = kabidhi, glossary UNVERIFIED). Subject concord assumes 'vocha/motisha' unspecified; may need adjusting. |
| `auditTrail.empty` | Nothing recorded yet. | Bado hakuna kilichorekodiwa. | draft |  |
| `auditTrail.heldForAudit` | Held for an in-person audit | Imeshikiliwa kwa ukaguzi wa ana kwa ana | draft | Flag: Drafter: 'in-person audit' -> 'ukaguzi wa ana kwa ana' (composed; 'ukaguzi' UNVERIFIED). |
| `auditTrail.idSeen` | ID checked: {{type}} | Kitambulisho kilichokaguliwa: {{type}} | draft | Keep {{type}} |
| `auditTrail.idType.driving_licence` | Driving licence | Leseni ya udereva | draft | Flag: Drafter: 'Leseni ya udereva' UNVERIFIED in glossary. |
| `auditTrail.idType.nida` | NIDA card | Kitambulisho cha Taifa (NIDA) | draft |  |
| `auditTrail.idType.village_letter` | Village letter | Barua ya kijiji | draft | Flag: Drafter: 'Barua ya kijiji' is a composed name for the ID-substitute document; exact Tanzanian name unknown. |
| `auditTrail.idType.voter` | Voter card | Kadi ya mpiga kura | draft |  |
| `auditTrail.kind.answered` | Survey answered | Dodoso limejibiwa | draft |  |
| `auditTrail.kind.household_registered` | Household registered | Kaya imesajiliwa | draft |  |
| `auditTrail.kind.household_verified` | Household verified | Kaya imehakikiwa | draft |  |
| `auditTrail.kind.issued` | Voucher issued | Vocha imetolewa | draft |  |
| `auditTrail.kind.login_initial` | App login created | Akaunti ya programu imeundwa | draft |  |
| `auditTrail.kind.login_reset` | App password reset | Neno la siri la programu limerudishwa | draft |  |
| `auditTrail.kind.redeemed` | Incentive handed over | Motisha imekabidhiwa | draft |  |
| `auditTrail.kind.refused` | Scan refused | Skani imekataliwa | draft | Flag: Drafter: 'Scan refused' -> 'Skani imekataliwa'; glossary suggests 'imezuiwa' for policy blocks. Chose imekataliwa to match 'refused'. |
| `auditTrail.kind.scanned` | Voucher scanned | Vocha imeskaniwa | draft | Flag: Drafter: 'imeskaniwa' passive of loan 'skani'; unattested. |
| `auditTrail.kind.survey_published` | Survey published | Dodoso limechapishwa | draft | Flag: Drafter: 'published' -> 'limechapishwa'; 'kuchapisha' is print/publish, may read as printed. Alternative 'limetolewa'. Cross-check (natural): 'limechapishwa' back-translates as 'printed/issued'; for a survey made live in the app it may read as printed on paper (drafter also flagged). |
| `auditTrail.kind.voided` | Voucher cancelled | Vocha imeghairiwa | draft |  |
| `auditTrail.reason` | Reason: {{reason}} | Sababu: {{reason}} | draft | Keep {{reason}} |
| `auditTrail.role.admin` | Admin | Msimamizi | draft | Flag: Drafter: 'Msimamizi' (glossary UNVERIFIED). |
| `auditTrail.role.farmer` | Farmer | Mkulima | draft |  |
| `auditTrail.role.field_officer` | Field officer | Afisa wa uwandani | draft | Flag: Drafter: 'Afisa wa uwandani' per glossary decision (composed, unverified). |
| `auditTrail.role.ops` | Ops | Uendeshaji | draft | Flag: Drafter: 'Ops' role -> 'Uendeshaji' (glossary UNVERIFIED, means 'operations'). |
| `auditTrail.role.unknown` | Unknown role | Jukumu halijulikani | draft |  |
| `auditTrail.title` | Audit trail | Kumbukumbu za ukaguzi | draft | Flag: Drafter: 'Audit trail' = 'Kumbukumbu za ukaguzi' (glossary UNVERIFIED). Cross-check (natural): 'Kumbukumbu za ukaguzi' back-translates as 'audit records (records of checking)'; it reads as a list of inspections rather than a who-did-what trail. Glossary-locked, so only for the native reviewer. |
| `capacityBasis.nameplate` | Nameplate | Kibao cha mashine | draft | Capacity is PLANNED or NAMEPLATE, never measured. Always shown with its basis. Flag: Drafter: 'Kibao cha mashine' (machine plate) is a short label; glossary only has a long unverified phrase. Must stay distinct from 'Uliopangwa' and never read as measured. Cross-check (meaning): 'Kibao cha mashine' (back: 'Machine plate') names the object, not a rated/nameplate figure, and 'kibao' alone can mean signboard or tablet; it may not read as a stated rating that is not measured. Native/energy reviewer should confirm. |
| `capacityBasis.planned` | Planned | Uliopangwa | draft | Capacity is PLANNED or NAMEPLATE, never measured. Always shown with its basis. Flag: Drafter: 'Uliopangwa' agrees with 'uwezo' (capacity), since this label sits next to a capacity figure. May need a different class agreement if used beside another noun. |
| `common.back` | Back | Rudi nyuma | draft |  |
| `common.close` | Close | Funga | draft |  |
| `common.loading` | Loading… | Inapakia… | draft | Flag: Drafter: 'Inapakia…' unverified per glossary. |
| `common.no` | No | Hapana | draft |  |
| `common.optional` | optional | si lazima | draft | Flag: Drafter: 'si lazima' unverified per glossary (alt 'hiari'). |
| `common.yes` | Yes | Ndiyo | draft |  |
| `confidence.high` | High | Juu | draft | Confidence level recorded with a figure. Low / medium / high. |
| `confidence.low` | Low | Chini | draft | Confidence level recorded with a figure. Low / medium / high. Flag: Drafter: 'Chini/Wastani/Juu' scale words not attested for confidence. |
| `confidence.medium` | Medium | Wastani | draft | Confidence level recorded with a figure. Low / medium / high. |
| `cycleDetail.harvests` | Harvest figures | Kiasi cha mavuno | draft |  |
| `cycleDetail.measure` | Planted | Kiasi kilichopandwa | draft | Flag: Drafter: Label 'Planted' whose value is area, tree count or unit count; 'Kilichopandwa' composed. Cross-check (meaning): "Kilichopandwa" reads as "What was planted", which a farmer may take to mean the crop name, but the value is a quantity (area, trees or units). |
| `cycleDetail.noHarvestsDetail` | Nobody has recorded an expected or actual figure for this cycle. | Hakuna aliyerekodi kiasi kinachotarajiwa wala halisi kwa msimu huu wa zao. | draft | Flag: Drafter: 'cycle' = 'msimu wa zao' (glossary UNVERIFIED). |
| `cycleDetail.noHarvestsTitle` | No harvest figure yet | Bado hakuna kiasi cha mavuno | draft |  |
| `cycleDetail.notFoundDetail` | It may not exist, or it may be outside your villages. | Huenda haupo, au uko nje ya vijiji vyako. | draft |  |
| `cycleDetail.notFoundTitle` | Crop cycle not found | Msimu wa zao haukupatikana | draft |  |
| `cycleDetail.season` | Season | Msimu | draft | Flag: Drafter: 'Season' = 'Msimu'; free-text value from officer, taxonomy open (S22). |
| `cycleDetail.seriesNote` | A harvest figure is a series. A revised estimate does not erase the one it replaced. | Kiasi cha mavuno ni mfululizo wa rekodi. Makadirio yaliyorekebishwa hayafuti yale ya awali. | draft |  |
| `cycleDetail.status` | Status | Hali | draft |  |
| `cycleDetail.superseded` | superseded | imebadilishwa na mpya | draft | Flag: Replaced after the cross-check. Meaning: this figure was replaced by a newer one and is excluded from totals. Drafter: 'imepitwa na mpya' (overtaken by a newer one) is my composition; agreement varies with the noun. Cross-check (meaning): "imepitwa na mpya" was read as "overtaken by a new one"; it does not clearly say the figure was replaced and is excluded from totals (superseded), and the agreement is fixed to one noun class. |
| `cycleDetail.trees_one` | 1 tree | mti 1 | draft |  |
| `cycleDetail.trees_other` | {{count}} trees | miti {{count}} | draft | Keep {{count}} |
| `cycleDetail.units_one` | 1 unit | kitengo 1 | draft | Flag: Drafter: 'Unit' as a crop count (not equipment) rendered 'kitengo'/'vitengo'; unsure what the unit is (e.g. hives, beds). Also unused in source. |
| `cycleDetail.units_other` | {{count}} units | vitengo {{count}} | draft | Keep {{count}} |
| `cycleDetail.window` | Harvest window | Kipindi cha kuvuna | draft |  |
| `cycleStatus.abandoned` | Abandoned | Imeachwa | draft | Flag: Drafter: 'Imeachwa' for Abandoned cycle; plain word, not in glossary. |
| `cycleStatus.growing` | Growing | Inakua | draft |  |
| `cycleStatus.harvested` | Harvested | Imevunwa | draft |  |
| `cycleStatus.planned` | Planned | Imepangwa | draft |  |
| `dbError.addPhoneFirst` | Add a phone number before issuing a login. | Ongeza namba ya simu kabla ya kutoa akaunti ya kuingia. | draft | Flag: Drafter: 'Login' (issuing a login) rendered 'akaunti ya kuingia' (composed); consistent across personHasLogin, phoneHasLogin. Glossary has 'kadi ya kuingia' for the printed card. |
| `dbError.choosePasswordFirst` | Choose a new password first. | Chagua neno la siri jipya kwanza. | draft |  |
| `dbError.confirmIdName` | Confirm that the name on the ID matches the household. | Thibitisha kwamba jina kwenye kitambulisho linalingana na kaya. | draft |  |
| `dbError.cropNeedsArea` | This crop is measured by area, so the planted area is required. | Zao hili hupimwa kwa eneo, kwa hiyo eneo lililopandwa linahitajika. | draft |  |
| `dbError.cropNeedsTrees` | This crop is measured by tree count, so the number of trees is required. | Zao hili hupimwa kwa idadi ya miti, kwa hiyo idadi ya miti inahitajika. | draft |  |
| `dbError.cropNeedsUnits` | This crop is measured by unit count, so the number of units is required. | Zao hili hupimwa kwa idadi ya vitengo, kwa hiyo idadi ya vitengo inahitajika. | draft | Flag: Drafter: 'Unit count' rendered 'idadi ya vitengo' (composed). |
| `dbError.farmLocationRequired` | The farm location is required: capture the GPS position before saving. | Mahali shamba lilipo panahitajika: rekodi mahali kwa kutumia GPS kabla ya kuhifadhi. | draft | Flag: Drafter: 'Capture the GPS position' rendered 'chukua nafasi ya GPS' (composed; 'nafasi' = position/space). Reviewer to check. |
| `dbError.householdNeedsChecking` | Your household record needs checking by an officer. | Rekodi ya kaya yako inahitaji kukaguliwa na afisa. | draft | Flag: Drafter: 'Checking' rendered 'kukaguliwa' (not 'kuhakikiwa') to keep it distinct from formal verification; reviewer may prefer hakiki. |
| `dbError.householdNeedsSecondVerifier` | Your household must be verified by a second staff member. | Kaya yako lazima ihakikiwe na mfanyakazi wa pili. | draft | Flag: Drafter: 'Staff member' rendered 'mfanyakazi' (not in glossary). Also used in voucher and staffPasswordNotHere strings. |
| `dbError.householdNotVerified` | Your household has not been verified yet. | Kaya yako bado haijahakikiwa. | draft |  |
| `dbError.notAwaitingVerification` | That record is not awaiting verification. | Rekodi hiyo haisubiri kuhakikiwa. | draft |  |
| `dbError.notLinkedToFarmer` | Your account is not linked to a registered farmer. | Akaunti yako haijaunganishwa na mkulima aliyesajiliwa. | draft |  |
| `dbError.personNotFound` | That person was not found, or is outside your villages. | Mtu huyo hakupatikana, au yuko nje ya vijiji vyako. | draft |  |
| `dbError.phoneFormat` | The phone number must be a Tanzanian mobile number, for example +255 712 345 678. | Namba ya simu lazima iwe namba ya simu ya mkononi ya Tanzania, kwa mfano +255 712 345 678. | draft |  |
| `dbError.phoneHasLogin` | This phone number already has an app login. | Namba hii ya simu tayari ina akaunti ya kuingia kwenye programu. | draft |  |
| `dbError.phoneRequired` | A phone number is required so the farmer can sign in. | Namba ya simu inahitajika ili mkulima aweze kuingia. | draft |  |
| `dbError.questionChooseOne` | Question {{position}}: choose one of the listed options. | Swali {{position}}: chagua mojawapo ya chaguo zilizoorodheshwa. | draft | Keep {{position}} |
| `dbError.questionChooseSome` | Question {{position}}: choose from the listed options. | Swali {{position}}: chagua kutoka kwenye chaguo zilizoorodheshwa. | draft | Keep {{position}} |
| `dbError.questionEnterNumber` | Question {{position}}: enter a number. | Swali {{position}}: andika namba. | draft | Keep {{position}} |
| `dbError.questionRequired` | Question {{position}} is required. | Swali {{position}} ni lazima. | draft | Keep {{position}} |
| `dbError.questionTooLong` | Question {{position}}: answer in at most 2000 characters. | Swali {{position}}: jibu lisizidi herufi 2000. | draft | Keep {{position}} |
| `dbError.questionYesNo` | Question {{position}}: answer yes or no. | Swali {{position}}: jibu ndiyo au hapana. | draft | Keep {{position}} |
| `dbError.recordIdDocument` | Record which ID document you checked. | Andika aina ya kitambulisho ulichokagua. | draft |  |
| `dbError.recordNotFound` | That record was not found, or it is outside your villages. | Rekodi hiyo haikupatikana, au iko nje ya vijiji vyako. | draft |  |
| `dbError.redeemOwnHousehold` | You cannot redeem a voucher for your own household. | Huwezi kukabidhi vocha ya kaya yako mwenyewe. | draft |  |
| `dbError.redeemOwnRegistration` | You registered this household, so another staff member must redeem this voucher. | Ulisajili kaya hii, kwa hiyo mfanyakazi mwingine lazima akabidhi vocha hii. | draft |  |
| `dbError.redeemOwnVerification` | You verified this household, so another staff member must redeem this voucher. | Ulihakiki kaya hii, kwa hiyo mfanyakazi mwingine lazima akabidhi vocha hii. | draft |  |
| `dbError.setOwnPasswordFirst` | Set your own password before answering surveys. | Weka neno la siri lako mwenyewe kabla ya kujibu madodoso. | draft |  |
| `dbError.signInFirst` | Sign in first. | Ingia kwanza. | draft |  |
| `dbError.staffPasswordNotHere` | This person signs in as staff, so their password cannot be reset here. | Mtu huyu huingia kama mfanyakazi, kwa hiyo neno lake la siri haliwezi kurudishwa hapa. | draft | Flag: Cross-check (natural): 'kurudishwa' alone reads as 'returned', not 'reset' (back-translation lost 'reset'); follows the glossary 'Rudisha neno la siri' but is ambiguous in a sentence. |
| `dbError.surveyClosed` | This survey is closed. | Dodoso hili limefungwa. | draft |  |
| `dbError.surveyLimitReached` | This survey has reached its limit. | Dodoso hili limefikia kikomo chake. | draft |  |
| `dbError.surveyNotFound` | This survey could not be found. | Dodoso hili halikupatikana. | draft |  |
| `dbError.surveyNotInVillage` | This survey is not open in your village. | Dodoso hili halijafunguliwa katika kijiji chako. | draft |  |
| `dbError.surveyOpenedBeforeRegistration` | This survey opened before your household was registered. | Dodoso hili lilifunguliwa kabla kaya yako haijasajiliwa. | draft |  |
| `dbError.verifyOtherOfficer` | A household must be verified by someone other than the officer who registered it. | Kaya lazima ihakikiwe na mtu mwingine, si afisa aliyeisajili. | draft |  |
| `dbError.voucherAlreadyRedeemed` | This voucher was already redeemed on {{date}} by {{name}}. | Motisha ya vocha hii ilishakabidhiwa tarehe {{date}} na {{name}}. | draft | Keep {{date}} {{name}} Flag: Drafter: 'Redeemed' rendered 'imekabidhiwa' (handed over) for officer-facing errors; farmer-facing status elsewhere is 'imechukuliwa'. Confirm which wording the reviewer wants across the app. Applies also to ...ByOther, redeemOwn*, voucherHeldForAudit. Cross-check (meaning): 'Redeemed' is rendered 'kabidhiwa' (handed over), so the back-translation reads as the voucher itself being handed over rather than redeemed for cash; the same verb is used for ByOther, redeemOwnRegistration, redeemOwnVerification, redeemOwnHousehold, voucherHeldForAudit and tour.officer.redeemBody, and differs from the farmer-facing 'imechukuliwa'. |
| `dbError.voucherAlreadyRedeemedByOther` | This voucher was already redeemed on {{date}} by another staff member. | Motisha ya vocha hii ilishakabidhiwa tarehe {{date}} na mfanyakazi mwingine. | draft | Keep {{date}} Flag: Changed after the cross-check; unreviewed. |
| `dbError.voucherExpired` | This voucher expired on {{date}}. | Vocha hii iliisha muda wake tarehe {{date}}. | draft | Keep {{date}} Flag: Drafter: 'iliisha muda wake' from glossary 'imeisha muda wake' [UNVERIFIED]. |
| `dbError.voucherHeldForAudit` | This voucher is held for an audit: ops or admin must redeem it in person. | Vocha hii imeshikiliwa kwa ukaguzi: mtu wa uendeshaji au msimamizi lazima aikabidhi ana kwa ana. | draft |  |
| `dbError.voucherNotFound` | Voucher not found. | Vocha haikupatikana. | draft |  |
| `dbError.voucherVoided` | This voucher was voided: {{reason}} | Vocha hii imebatilishwa: {{reason}} | draft | Keep {{reason}} Flag: Drafter: 'Voided' rendered 'imebatilishwa' (cancelled/annulled); not in glossary. |
| `demoBanner.detail` | Every figure here is invented. Nothing is a measured Ruaha result. | Kila takwimu hapa ni ya kubuni. Hakuna hata moja ambayo ni matokeo yaliyopimwa ya Ruaha. | draft | Flag: Drafter: Reworded to 'takwimu ... ya kubuni' (invented figures); native check that 'ya kubuni' reads as fictitious and not 'creative'. 'Si matokeo yaliyopimwa' keeps the not-measured meaning. |
| `demoBanner.label` | Demo data | Taarifa za majaribio | draft | Flag: Drafter: 'Taarifa za majaribio' unverified per glossary. |
| `draft.earlierVersionSaved` | An earlier version of this form was already saved. Open it from the list to check it. | Toleo la awali la fomu hii lilishahifadhiwa. Lifungue kutoka kwenye orodha ili kulikagua. | draft | Flag: Changed after the cross-check; unreviewed. |
| `draft.notSubmitted` | Not yet submitted | Haijawasilishwa bado | draft | Flag: Drafter: 'Haijawasilishwa bado' from glossary, unsourced. |
| `draft.storageError` | Could not save this draft on this device. Keep this page open until you submit. | Rasimu hii haikuweza kuhifadhiwa kwenye kifaa hiki. Acha ukurasa huu wazi hadi uwasilishe. | draft | Flag: Drafter: 'Rasimu' (draft) and 'kifaa' (device) ; 'Acha ukurasa huu wazi' for 'Keep this page open' composed. |
| `empty.noAccessDetail` | You may not have access to these records, or there are none yet. | Huenda huna ruhusa ya kuona rekodi hizi, au bado hakuna. | draft |  |
| `empty.noAccessToThis` | Nothing to show | Hakuna cha kuonyesha | draft |  |
| `equipment.acquisition` | Rent or buy? | Kukodi au kununua? | draft |  |
| `equipment.acquisitionOnly.buy` | Offered to buy only. | Kinapatikana kwa kununua tu. | draft |  |
| `equipment.acquisitionOnly.rent` | Offered to rent only. | Kinapatikana kwa kukodi tu. | draft |  |
| `equipment.chooseAcquisition` | Choose whether you want to rent it or buy it. | Chagua kama unataka kukikodi au kukinunua. | draft |  |
| `equipment.days` | Days per week | Siku kwa wiki | draft |  |
| `equipment.daysDecimals` | Use at most 1 decimal place. | Tumia desimali isiyozidi 1. | draft |  |
| `equipment.daysRange` | Days per week run from 0 to 7. | Siku kwa wiki ni kuanzia 0 hadi 7. | draft |  |
| `equipment.estimateBlockedDetail` | Fill in how many, hours per day and days per week, and the estimate appears here. | Jaza idadi, saa kwa siku na siku kwa wiki, na makadirio yataonekana hapa. | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. |
| `equipment.estimateBlockedTitle` | No estimate yet | Bado hakuna makadirio | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. |
| `equipment.estimateImpossibleDetail` | One of the figures above is outside what is possible, so no estimate is shown for it. Correct it and the estimate returns. | Moja ya namba zilizo hapo juu iko nje ya kinachowezekana, kwa hiyo hakuna makadirio yanayoonyeshwa kwake. Irekebishe, na makadirio yatarudi. | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. Flag: Drafter: Idiom 'nje ya kinachowezekana' for 'outside what is possible' is composed; check naturalness. |
| `equipment.estimateImpossibleTitle` | This cannot be estimated | Hili haliwezi kukadiriwa | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. |
| `equipment.hours` | Hours per day | Saa kwa siku | draft |  |
| `equipment.hoursDecimals` | Use at most 2 decimal places. | Tumia desimali zisizozidi 2. | draft |  |
| `equipment.hoursRange` | Hours per day run from 0 to 24. | Saa kwa siku ni kuanzia 0 hadi 24. | draft |  |
| `equipment.indicative` | Indicative | Ya makadirio | draft | Prices are INDICATIVE, never quotations. Must not read as a firm offer. |
| `equipment.indicativePrice` | Indicative price | Bei ya makadirio | draft | Prices are INDICATIVE, never quotations. Must not read as a firm offer. |
| `equipment.intro` | Powered equipment available through the programme. | Vifaa vya umeme vinavyopatikana kupitia mradi. | draft | Flag: Drafter: 'Programme' = 'programu' also means 'app' in Tanzanian Swahili; reviewer to choose 'mpango' or 'programu'. Also 'powered equipment' = 'vifaa vya umeme' (assumed electric). Cross-check (natural): "programu" also means "app" in Tanzanian Swahili (the back-translator flagged both readings), and "vifaa vya umeme" narrows "powered equipment" to electric. The same applies to "programu" in equipment.noneDetail. |
| `equipment.moreThanZero` | This must be more than zero. | Hii lazima iwe zaidi ya sifuri. | draft |  |
| `equipment.noneDetail` | Equipment appears here once the programme adds it. | Vifaa vitaonekana hapa mradi utakapoviongeza. | draft | Flag: Cross-check (natural): "programu itakapoviongeza" can read as "when the app adds them". |
| `equipment.noneTitle` | No equipment listed yet | Bado hakuna vifaa vilivyoorodheshwa | draft |  |
| `equipment.notANumber` | Enter a number. | Weka namba. | draft |  |
| `equipment.notAQuotation` | Prices are indicative, not quotations. | Bei ni ya makadirio, si nukuu ya bei. | draft | Flag: Drafter: Kept 'si nukuu ya bei' per glossary; 'Bei ni ya makadirio' number agreement is generic. |
| `equipment.notFoundDetail` | It may no longer be listed, or you may not have access to it. | Huenda hakiorodheshwi tena, au huna ruhusa ya kukifikia. | draft |  |
| `equipment.notFoundTitle` | Equipment not found | Kifaa hakikupatikana | draft |  |
| `equipment.price` | Price | Bei ya makadirio | draft | Prices are INDICATIVE, never quotations. Must not read as a firm offer. Flag: Drafter: Labelled 'Bei ya makadirio' (glossary decision) because the note requires indicative wording; English label is just 'Price'. |
| `equipment.purpose` | What will you use it for? | Utakitumia kwa ajili ya nini? | draft | Flag: Drafter: Assumes equipment noun class 7 ('kifaa' -> 'Utakitumia'). |
| `equipment.quantity` | How many | Idadi | draft |  |
| `equipment.ratedPower` | Rated power | Nguvu iliyoandikwa | draft | Flag: Cross-check (glossary): Same as estimate.ratedPower: shortened nameplate term "nguvu iliyoandikwa" is ambiguous, and "iliyoandikwa iliyorekodiwa" in requests.noEstimateDetail is clumsy. |
| `equipment.requestThis` | Request this | Omba kifaa hiki | draft |  |
| `equipment.required` | This is required. | Ni lazima. | draft | Flag: Drafter: 'Ni lazima.' short form; the English 'This is required.' has no clear subject. |
| `equipment.submit` | Submit request | Wasilisha ombi | draft | Flag: Drafter: Submit vs send: 'Wasilisha ombi' here (matches status 'Imewasilishwa') but 'Tuma ombi hili' for requests.action.submit and 'haijatumwa' in draftNote, following glossary Tuma=send. Reviewer to unify. |
| `equipment.submitting` | Submitting… | Inawasilisha… | draft |  |
| `equipment.successDetail` | Ops will review it. You can follow it under Requests. | Timu ya uendeshaji italipitia. Unaweza kulifuatilia chini ya Maombi. | draft | Flag: Drafter: 'Ops' rendered 'Timu ya uendeshaji' (glossary 'Uendeshaji' role UNVERIFIED). |
| `equipment.successTitle` | Request submitted | Ombi limewasilishwa | draft |  |
| `equipment.title` | Equipment | Vifaa | draft |  |
| `equipment.viewRequest` | View the request | Tazama ombi | draft |  |
| `equipment.wholeNumber` | Enter a whole number. | Weka namba nzima. | draft |  |
| `error.accountUnreadable` | Your account could not be read. Try again. | Akaunti yako haikuweza kusomwa. Jaribu tena. | draft |  |
| `error.badId` | That record reference is not valid. | Rejea ya rekodi hiyo si sahihi. | draft |  |
| `error.badReference` | That refers to a record that does not exist. | Hiyo inarejelea rekodi ambayo haipo. | draft |  |
| `error.contributionPositive` | A contribution must be more than zero. | Kiasi kinachochangiwa lazima kiwe zaidi ya sifuri. | draft | Flag: Drafter: 'Contribution' (quantity attached from a harvest figure to an opportunity) rendered 'kiasi kinachochangiwa' (composed). |
| `error.daysRange` | Days per week must be between 0 and 7. | Siku kwa wiki lazima ziwe kati ya 0 na 7. | draft |  |
| `error.duplicate` | That record already exists. | Rekodi hiyo tayari ipo. | draft |  |
| `error.duplicateBuyer` | A buyer with that name already exists in this project. | Mnunuzi mwenye jina hilo tayari yupo katika mradi huu. | draft |  |
| `error.duplicateEquipmentCode` | That equipment code is already in use in this project. | Msimbo huo wa kifaa tayari unatumika katika mradi huu. | draft |  |
| `error.duplicateLoanCode` | That loan code is already in use in this project. | Msimbo huo wa mkopo tayari unatumika katika mradi huu. | draft |  |
| `error.duplicateSupply` | That harvest figure is already attached to this opportunity. | Kiasi hicho cha mavuno tayari kimeambatishwa kwenye fursa hii. | draft |  |
| `error.duplicateVillageCode` | A village with that code already exists in this project. | Kijiji chenye msimbo huo tayari kipo katika mradi huu. | draft |  |
| `error.equipmentOffered` | Equipment must be offered to rent, to buy, or both. | Kifaa lazima kitolewe kwa kukodi, kwa kununua, au vyote viwili. | draft |  |
| `error.hoursRange` | Hours per day must be between 0 and 24. | Saa kwa siku lazima ziwe kati ya 0 na 24. | draft |  |
| `error.invalidValue` | One of the values is not allowed here. Check the form and try again. | Moja ya thamani haikubaliki hapa. Angalia fomu kisha ujaribu tena. | draft |  |
| `error.loanRange` | The smallest amount must be more than zero and no larger than the largest. | Kiasi cha chini lazima kiwe zaidi ya sifuri na kisizidi kiasi cha juu. | draft |  |
| `error.missingValue` | A required value is missing. | Thamani inayohitajika haipo. | draft |  |
| `error.network` | Could not reach the server. Check your connection and try again. | Imeshindwa kuwasiliana na seva. Angalia mtandao wako kisha ujaribu tena. | draft |  |
| `error.notAllowed` | You do not have permission to do that. | Huna ruhusa ya kufanya hivyo. | draft |  |
| `error.numericOverflow` | One of the numbers is too large for the field it was typed into. Check the figures and try again. | Moja ya namba ni kubwa mno kwa kisanduku ulichoiandikia. Angalia namba kisha ujaribu tena. | draft |  |
| `error.oneCurrentHarvest` | A crop cycle can only have one current harvest figure. Supersede the existing one instead. | Msimu wa zao unaweza kuwa na kiasi kimoja tu cha sasa cha mavuno. Badala yake, weka kiasi kipya kuchukua nafasi ya kilichopo. | draft | Flag: Drafter: 'Supersede' rendered 'chukua nafasi ya kilichopo kwa kiasi kipya' (replace the existing with a new figure); no glossary term. |
| `error.personHasLogin` | This person already has an app login. | Mtu huyu tayari ana akaunti ya kuingia kwenye programu. | draft |  |
| `error.quantityNotNegative` | The quantity cannot be negative. | Idadi haiwezi kuwa chini ya sifuri. | draft |  |
| `error.quantityPositive` | The quantity must be more than zero. | Idadi lazima iwe zaidi ya sifuri. | draft |  |
| `error.retry` | Try again | Jaribu tena | draft |  |
| `error.surveyAlreadyAnswered` | Your household has already answered this survey. | Kaya yako tayari imejibu dodoso hili. | draft |  |
| `error.title` | Something went wrong | Kuna tatizo | draft | Flag: Drafter: 'Kuna tatizo' [UNVERIFIED glossary term]; 'error.retry' uses 'Jaribu tena' [UNVERIFIED]. |
| `error.unexpected` | Something went wrong that should not have. Try again, and tell the programme team if it keeps happening. | Kumetokea hitilafu isiyotarajiwa. Jaribu tena, na uiambie timu ya mradi ikiendelea kutokea. | draft | Flag: Drafter: 'Programme team' rendered 'timu ya programu'; sentence composed [UNVERIFIED like 'Kuna tatizo' glossary row]. |
| `error.windowBackwards` | The window must end on or after it starts. | Kipindi lazima kiishe siku ile ile kinapoanza au baada yake. | draft | Flag: Drafter: 'Window' rendered 'kipindi' (glossary: harvest window = kipindi cha kuvuna); the error does not say which kind of window, so check context. |
| `estimate.aDay` | a day | kwa siku | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. |
| `estimate.aWeek` | a week | kwa wiki | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. |
| `estimate.basis` | Calculated from rated power and the operating assumptions below (Level 1). | Imekokotolewa kutokana na nguvu iliyoandikwa na dhana za matumizi zilizo hapa chini (Kiwango cha 1). | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. Flag: Drafter: 'operating assumptions' = 'dhana za matumizi', 'Level 1' = 'Kiwango cha 1'; both composed. 'Rated power' = 'nguvu iliyoandikwa' (nameplate, glossary UNVERIFIED). |
| `estimate.days` | Days per week | Siku kwa wiki | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. |
| `estimate.daysAWeek` | days a week | siku kwa wiki | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. |
| `estimate.hours` | Hours per day | Saa kwa siku | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. |
| `estimate.hoursADay` | hours a day | saa kwa siku | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. |
| `estimate.isEstimate` | This is an estimate, not a measurement. | Haya ni makadirio, si kipimo. | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. |
| `estimate.peak` | Estimated peak | Kilele kilichokadiriwa | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. Flag: Drafter: 'Kilele kilichokadiriwa' composed (glossary UNVERIFIED); estimate.peakNote uses 'nguvu ya juu zaidi' for peak power, also UNVERIFIED. |
| `estimate.peakNote` | Peak power does not change with hours of use. | Nguvu ya juu zaidi haibadiliki kulingana na saa za matumizi. | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. |
| `estimate.perDay` | Estimated per day | Makadirio kwa siku | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. |
| `estimate.perWeek` | Estimated per week | Makadirio kwa wiki | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. |
| `estimate.quantity` | Quantity | Idadi | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. |
| `estimate.rated` | rated | iliyoandikwa | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. Flag: Drafter: Appears as a unit label after a kW figure ('15 kW iliyoandikwa'); glossary nameplate wording shortened. Needs energy reviewer. |
| `estimate.ratedPower` | Rated power | Nguvu iliyoandikwa | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. Flag: Cross-check (glossary): The glossary nameplate wording "nguvu iliyoandikwa kwenye kibao cha mashine" was cut to "nguvu iliyoandikwa", which the blind translator read as "written-down (rated) power" and which loses the idea of the manufacturer rating on the equipment. The same short form is used in estimate.basis and estimate.rated. |
| `estimate.tag` | Estimate | Makadirio | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. |
| `estimate.title` | Energy estimate | Makadirio ya nishati | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. |
| `estimate.units_one` | unit | kifaa | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. Flag: Drafter: Unit noun for equipment items ('kifaa'/'vifaa'). Unused in source (usedIn empty); appears after a number in the equation. |
| `estimate.units_other` | units | vifaa | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. |
| `farmDetail.gps` | GPS point | Alama ya GPS | draft |  |
| `farmDetail.noGps` | Not captured | Haijarekodiwa | draft | Flag: Drafter: 'Haijachukuliwa' (not captured); could read as 'not taken'. Alternative 'Haijarekodiwa'. |
| `farmDetail.noPlotsDetail` | This farm has no plots yet. | Shamba hili bado halina vipande vya shamba. | draft |  |
| `farmDetail.noPlotsTitle` | No plots recorded | Hakuna kipande cha shamba kilichorekodiwa | draft |  |
| `farmDetail.notFoundDetail` | It may not exist, or it may be outside your villages. | Huenda halipo, au liko nje ya vijiji vyako. | draft |  |
| `farmDetail.notFoundTitle` | Farm not found | Shamba halikupatikana | draft |  |
| `farmDetail.plotCount` | Plots | Vipande vya shamba | draft |  |
| `farmDetail.plots` | Plots | Vipande vya shamba | draft |  |
| `farmerLogin.cancel` | Cancel | Ghairi | draft | Flag: Drafter: 'Ghairi' is UNVERIFIED in glossary; also used in voucherStatus.void 'Imeghairiwa' and voucher.voided/auditTrail.kind.voided. |
| `farmerLogin.detail` | Give these to the farmer. They sign in with their phone number and must choose their own password straight away. | Mpe mkulima taarifa hizi. Ataingia kwa namba yake ya simu na lazima achague neno lake la siri papo hapo. | draft | Flag: Changed after the cross-check; unreviewed. |
| `farmerLogin.done` | Done | Maliza | draft | Flag: Drafter: 'Done' button rendered 'Maliza' (glossary UNVERIFIED). |
| `farmerLogin.failed` | The farmer is registered, but the app login could not be created. | Mkulima amesajiliwa, lakini akaunti ya programu haikuweza kuundwa. | draft |  |
| `farmerLogin.historyInitial` | Login created by {{name}} | Akaunti iliundwa na {{name}} | draft | Keep {{name}} |
| `farmerLogin.historyNone` | No app login yet. | Bado hakuna akaunti ya programu. | draft |  |
| `farmerLogin.historyReset` | Password reset by {{name}} | Neno la siri lilirudishwa na {{name}} | draft | Keep {{name}} |
| `farmerLogin.historyTitle` | App login | Akaunti ya programu | draft |  |
| `farmerLogin.issue` | Create app login | Unda akaunti ya programu | draft |  |
| `farmerLogin.issuing` | Creating login… | Inaunda akaunti… | draft |  |
| `farmerLogin.mustChange` | Waiting for the farmer to choose their own password. | Inasubiri mkulima achague neno lake la siri mwenyewe. | draft |  |
| `farmerLogin.password` | Temporary password | Neno la siri la muda | draft |  |
| `farmerLogin.phone` | Phone number | Namba ya simu | draft |  |
| `farmerLogin.reset` | Reset app password | Rudisha neno la siri la programu | draft |  |
| `farmerLogin.resetConfirm` | Reset password | Rudisha neno la siri | draft |  |
| `farmerLogin.resetConfirmDetail` | Their current password stops working and anyone signed in with it is signed out. You will get a new temporary password to give them. | Neno lake la siri la sasa litaacha kufanya kazi, na yeyote aliyeingia nalo atatolewa kwenye programu. Utapata neno la siri la muda jipya la kumpa. | draft | Flag: Drafter: 'new temporary password' written 'neno jipya la siri la muda' (jipya agrees with neno). Glossary 'neno la siri jipya' agrees with the closer noun; reviewer to settle agreement. |
| `farmerLogin.resetConfirmTitle` | Reset this farmer's app password? | Rudisha neno la siri la programu la mkulima huyu? | draft |  |
| `farmerLogin.resetting` | Resetting… | Inarudisha… | draft |  |
| `farmerLogin.retry` | Try again | Jaribu tena | draft |  |
| `farmerLogin.shownOnce` | Shown once. Write it down for the farmer now; it cannot be shown again. | Huonyeshwa mara moja. Liandike sasa kwa ajili ya mkulima; haliwezi kuonyeshwa tena. | draft | Flag: Drafter: Object concord 'li-' refers to 'neno la siri'; check reads naturally to a farmer/officer. |
| `farmerLogin.title` | App login for the farmer | Akaunti ya programu ya mkulima | draft | Flag: Drafter: 'App login' rendered as 'Akaunti ya programu' (app account). 'login' as a noun has no glossary term; also affects issue/history/audit strings. 'programu' for 'app' is common but unverified. |
| `farmerOpportunities.buyerWithOps` | Your field officer holds the buyer details. | Afisa wako wa uwandani ana taarifa za mnunuzi. | draft |  |
| `farmerOpportunities.noneDetail` | None of your harvest has been put forward to a buyer yet. | Bado hakuna mavuno yako yaliyopendekezwa kwa mnunuzi. | draft |  |
| `farmerOpportunities.noneTitle` | No opportunities yet | Bado hakuna fursa | draft |  |
| `farmerOpportunities.notASale` | An opportunity is not a sale, a delivery or a payment. It records that your harvest has been put forward to a buyer. Nothing has moved and nothing is owed. | Fursa si mauzo, si uwasilishaji wa mazao wala si malipo. Inaonyesha kwamba mavuno yako yamependekezwa kwa mnunuzi. Hakuna kilichohamishwa na hakuna deni lolote. | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. Flag: Drafter: Glossary bans 'mauzo' for the opportunity itself, but the negation 'not a sale' needs a word for sale; 'mauzo' used only inside the negation. 'delivery' rendered 'uwasilishaji wa mazao' (collides with 'wasilisha'=submit). 'Nothing is owed' = 'hakuna anayedaiwa chochote' unverified. Same in farmHome.notASale. |
| `farmerOpportunities.opportunityTotal` | Opportunity total | Jumla katika fursa hii | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. Flag: Cross-check (meaning): "Jumla ya fursa" can be read as the total number of opportunities, not the total quantity within this one opportunity. |
| `farmerOpportunities.title` | Opportunities | Fursa | draft |  |
| `farmerOpportunities.yourShare` | Your share | Sehemu yako | draft |  |
| `farmHome.browseEquipment` | Browse equipment | Angalia vifaa | draft |  |
| `farmHome.cycles` | Crops growing | Mazao yanayokua | draft |  |
| `farmHome.farms` | Farms | Mashamba | draft |  |
| `farmHome.latestRequest` | Latest equipment request | Ombi la karibuni la vifaa | draft |  |
| `farmHome.noFarmDetail` | Your field officer has not registered your farm yet. Ask them to add it. | Afisa wako wa uwandani bado hajasajili shamba lako. Mwombe alisajili. | draft |  |
| `farmHome.noFarmTitle` | No farm recorded yet | Bado hakuna shamba lililorekodiwa | draft |  |
| `farmHome.noOpportunities` | None of your harvest has been put forward to a buyer yet. | Bado hakuna mavuno yako yaliyopendekezwa kwa mnunuzi. | draft |  |
| `farmHome.noRequests` | You have not asked for any equipment yet. | Bado hujaomba kifaa chochote. | draft |  |
| `farmHome.notASale` | An opportunity is not a sale, a delivery or a payment. | Fursa si mauzo, si uwasilishaji wa mazao wala si malipo. | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. Flag: Drafter: Same as farmerOpportunities.notASale: 'mauzo' appears only in the negation. |
| `farmHome.openMyFarm` | See my records | Tazama taarifa zangu | draft |  |
| `farmHome.openOpportunities` | See opportunities | Tazama fursa | draft |  |
| `farmHome.opportunities` | Opportunities | Fursa | draft |  |
| `farmHome.opportunityCount_one` | Your harvest is in 1 opportunity. | Mavuno yako yamo katika fursa 1. | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `farmHome.opportunityCount_other` | Your harvest is in {{count}} opportunities. | Mavuno yako yamo katika fursa {{count}}. | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. Keep {{count}} |
| `farmHome.plots` | Plots | Vipande vya shamba | draft |  |
| `farmHome.title` | My farm | Shamba langu | draft |  |
| `harvestKind.actual` | Actual | Halisi | draft |  |
| `harvestKind.expected` | Expected | Yanayotarajiwa | draft |  |
| `language.en` | English | Kiingereza | reviewed |  |
| `language.notSaved` | Language changed for now, but could not be saved. | Lugha imebadilishwa kwa sasa, lakini haikuweza kuhifadhiwa. | draft |  |
| `language.sw` | Kiswahili | Kiswahili | reviewed |  |
| `login.email` | Phone number or email | Namba ya simu au barua pepe | draft | Flag: Drafter: Key name says email but string is 'phone number or email'; 'barua pepe' is standard for email. |
| `login.emailInvalid` | Enter a Tanzanian mobile number, for example 0712 345 678, or an email address. | Weka namba ya simu ya mkononi ya Tanzania, kwa mfano 0712 345 678, au anwani ya barua pepe. | draft | Flag: Drafter: Example number 0712 345 678 kept as is. |
| `login.emailRequired` | Enter your phone number or email address. | Weka namba yako ya simu au anwani ya barua pepe. | draft |  |
| `login.invalid` | That phone number or email and password do not match an account. | Namba ya simu au barua pepe na neno la siri havilingani na akaunti yoyote. | draft |  |
| `login.network` | Could not reach the server. Check your connection and try again. | Seva haikupatikana. Angalia mtandao wako kisha ujaribu tena. | draft |  |
| `login.password` | Password | Neno la siri | draft |  |
| `login.passwordRequired` | Enter your password. | Weka neno lako la siri. | draft |  |
| `login.submit` | Sign in | Ingia | draft |  |
| `login.submitting` | Signing in… | Inaingia… | draft |  |
| `login.title` | Sign in | Ingia | draft |  |
| `login.unexpected` | Something went wrong signing in. | Kuna tatizo wakati wa kuingia. | draft |  |
| `myFarm.expected` | Expected harvest | Mavuno yanayotarajiwa | draft |  |
| `myFarm.noCycles` | No crops recorded on this plot yet | Bado hakuna zao lililorekodiwa katika kipande hiki | draft |  |
| `myFarm.noFarmDetail` | A field officer will register your farm and it will appear here. | Afisa wa uwandani atasajili shamba lako na litaonekana hapa. | draft |  |
| `myFarm.noFarmTitle` | No farm recorded yet | Bado hakuna shamba lililorekodiwa | draft |  |
| `myFarm.noPlots` | No plots recorded on this farm yet | Bado hakuna kipande cha shamba kilichorekodiwa katika shamba hili | draft |  |
| `myFarm.plots` | Plots | Vipande vya shamba | draft | Flag: Drafter: 'Kipande cha shamba' for plot is glossary UNVERIFIED; used throughout. |
| `myFarm.readOnly` | This is a record of what has been registered. Ask your field officer to change anything. | Hizi ni taarifa za kile kilichosajiliwa. Mwombe afisa wako wa uwandani abadilishe chochote. | draft |  |
| `myFarm.title` | My farm | Shamba langu | draft |  |
| `myFarm.window` | Harvest window | Kipindi cha kuvuna | draft |  |
| `nav.buyers` | Buyers | Wanunuzi | draft |  |
| `nav.catalogue` | Resources | Rasilimali | draft | Flag: Drafter: 'Rasilimali' (resources) not in glossary: unverified. |
| `nav.demand` | Demand | Mahitaji | draft |  |
| `nav.equipment` | Resources | Rasilimali | draft | Flag: Drafter: 'Rasilimali' (resources) not in glossary: unverified. Same word as nav.catalogue; shortened from 'Vifaa na mikopo' on 19 Oct 2026 so the tab label stays on one line. |
| `nav.myFarm` | My farm | Shamba | draft | Flag: Drafter: 'Shamba' (verified: farm) for the tab; shortened from 'Shamba langu' on 19 Oct 2026 because a two-word label wraps under the tab icon on a 375px phone. The screen heading keeps 'Shamba langu'. |
| `nav.opportunities` | Opportunities | Fursa | draft |  |
| `nav.people` | People | Watu | draft |  |
| `nav.redeem` | Redeem | Kabidhi | draft | Flag: Drafter: 'Kabidhi' (hand over) per glossary for the officer redeem action; unverified, sense is handing cash over at the office. Cross-check (meaning): 'Kabidhi' back-translates to 'Hand over', dropping the voucher/redeem sense; as a bare nav label it does not say what is being handed over or that it is a voucher claim at the office. |
| `nav.register` | Register | Sajili | draft |  |
| `nav.requests` | Requests | Maombi | draft |  |
| `nav.signingOut` | Signing out… | Inatoka… | draft | Flag: Drafter: 'Inatoka…' follows 'Toka'; could read as 'is leaving'. Alternative 'Inakutoa…' is not natural. |
| `nav.signOut` | Sign out | Toka | draft | Flag: Drafter: 'Toka' chosen, unverified (alt 'Ondoka'). |
| `nav.surveys` | Surveys | Madodoso | draft | Flag: Drafter: 'Madodoso' (plural of dodoso, questionnaire) used as the nav label; alternative 'Tafiti'. |
| `nav.tower` | Control Tower | Control Tower | draft | Flag: Drafter: Kept 'Control Tower' in English as a product/screen name; no glossary term. Reviewer may prefer a Swahili name. |
| `nav.userMenu` | Account menu | Menyu ya akaunti | draft |  |
| `nav.verify` | Verify | Hakiki | draft |  |
| `nav.villages` | Villages | Vijiji | draft |  |
| `noAccess.detail` | Your account is not attached to a project yet. Ask your programme manager to add you. | Akaunti yako bado haijaunganishwa na mradi. Muulize meneja wa mradi akuongeze. | draft | Flag: Changed after the cross-check; unreviewed. |
| `noAccess.nothingDetail` | This is a real screen with a next step — never a blank page, and never an error. | Hii ni skrini halisi yenye hatua inayofuata — kamwe si ukurasa mtupu, wala hitilafu. | draft | Flag: Drafter: Meta/developer-style English sentence; translated faithfully, native reviewer may simplify. |
| `noAccess.nothingTitle` | Nothing to show yet | Bado hakuna cha kuonyesha | draft |  |
| `noAccess.signOut` | Sign out | Toka | draft |  |
| `noAccess.title` | No access yet | Bado huna ufikiaji | draft | Flag: Drafter: 'ufikiaji' for 'access' not in glossary; plain wording. Alternative 'ruhusa'. |
| `notFound.detail` | The link may be out of date. | Huenda kiungo hicho kimepitwa na wakati. | draft |  |
| `notFound.home` | Go to your home screen | Nenda kwenye skrini yako ya mwanzo | draft |  |
| `notFound.title` | That page does not exist | Ukurasa huo haupo | draft |  |
| `officerEdit.action` | Edit | Hariri | draft | Flag: Drafter: 'Hariri' UNVERIFIED (MS guide only). 'Rekebisha' used for 'Correct harvest figure' per glossary. |
| `officerEdit.cancel` | Cancel | Ghairi | draft |  |
| `officerEdit.fields.confidence` | Confidence | Kiwango cha uhakika | draft | Confidence level recorded with a figure. Low / medium / high. |
| `officerEdit.fields.crop_id` | Crop | Zao | draft |  |
| `officerEdit.fields.cycle_area_ha` | Planted area (ha) | Eneo lililopandwa (ha) | draft | Flag: Same open question as register.cycleArea. Cross-check (glossary): The same English 'Planted area (ha)' (register.cycleArea, person.plantedArea) is 'Jumla ya eneo lililopandwa katika mazao yote' elsewhere but the short 'Eneo lililopandwa (ha)' here, so the across-cycles qualifier is dropped in the edit form and the same field has two labels. |
| `officerEdit.fields.family_name` | Family name | Jina la ukoo | draft |  |
| `officerEdit.fields.farm_label` | Farm name | Jina la shamba | draft |  |
| `officerEdit.fields.given_name` | First name | Jina la kwanza | draft |  |
| `officerEdit.fields.harvest_end` | Harvest window ends | Mwisho wa kipindi cha kuvuna | draft |  |
| `officerEdit.fields.harvest_start` | Harvest window starts | Mwanzo wa kipindi cha kuvuna | draft |  |
| `officerEdit.fields.household_label` | Household name | Jina la kaya | draft |  |
| `officerEdit.fields.latitude` | Latitude | Latitudo | draft |  |
| `officerEdit.fields.longitude` | Longitude | Longitudo | draft |  |
| `officerEdit.fields.phone` | Phone | Namba ya simu | draft |  |
| `officerEdit.fields.planted_on` | Planted on | Tarehe ya kupanda | draft |  |
| `officerEdit.fields.plot_area_ha` | Plot area (ha) | Eneo la kipande cha shamba (ha) | draft |  |
| `officerEdit.fields.plot_label` | Plot name | Jina la kipande cha shamba | draft |  |
| `officerEdit.fields.quantity_kg` | Quantity (kg) | Kiasi (kg) | draft | Flag: Drafter: 'Kiasi (kg)' per glossary for amounts. |
| `officerEdit.fields.reported_for` | Reported for | Taarifa kwa tarehe | draft | Flag: Cross-check said the first draft meant "date of the report". Replacement is not confident. Field means the date or period the figure applies to. Drafter: Field is a date; rendered 'Tarehe ya taarifa' (date of the report). Literal 'reported for' has no clean equivalent. Cross-check (meaning): 'Reported for' names the date or period the figure applies to. 'Tarehe ya taarifa' (date of the report) reads as the date the report was made, which is a different fact. |
| `officerEdit.fields.season_label` | Season | Msimu | draft | Flag: Drafter: 'Msimu' (season). Free-text field; season taxonomy open under S22. Cross-check (meaning): 'Season' is 'Msimu' while 'Crop cycle' is 'Msimu wa zao' (and 'Misimu ya mazao'). In the same crop-cycle edit form the two different fields end up with near-identical names, which is confusing. The glossary locks the cycle term, so this needs a native decision. |
| `officerEdit.fields.status` | Status | Hali | draft |  |
| `officerEdit.fields.tree_count` | Number of trees | Idadi ya miti | draft |  |
| `officerEdit.fields.unit_count` | Number of units | Idadi ya vitengo | draft |  |
| `officerEdit.save` | Save changes | Hifadhi mabadiliko | draft |  |
| `officerEdit.saving` | Saving… | Inahifadhi… | draft |  |
| `officerEdit.titles.crop_cycle` | Edit crop cycle | Hariri msimu wa zao | draft |  |
| `officerEdit.titles.farm` | Edit farm details | Hariri taarifa za shamba | draft |  |
| `officerEdit.titles.harvest_report` | Correct harvest figure | Rekebisha kiasi cha mavuno | draft |  |
| `officerEdit.titles.household` | Edit household details | Hariri taarifa za kaya | draft |  |
| `officerEdit.titles.person` | Edit farmer details | Hariri taarifa za mkulima | draft |  |
| `officerEdit.titles.plot` | Edit plot details | Hariri taarifa za kipande cha shamba | draft |  |
| `officerHome.farms` | Farms | Mashamba | draft |  |
| `officerHome.nothingOutstanding` | Every record in your villages is verified. | Rekodi zote katika vijiji vyako zimehakikiwa. | draft |  |
| `officerHome.noVillagesDetail` | Your account is on the project but not yet attached to a village. Ask your programme manager to assign one. | Akaunti yako iko kwenye mradi lakini bado haijaunganishwa na kijiji. Muulize meneja wa mradi akupangie kimoja. | draft | Flag: Changed after the cross-check; unreviewed. |
| `officerHome.noVillagesTitle` | No village assigned yet | Bado hujapangiwa kijiji | draft |  |
| `officerHome.openVerifyQueue` | Open the verify queue | Fungua orodha ya kuhakiki | draft |  |
| `officerHome.outstanding_one` | 1 record still needs verifying. | Rekodi 1 bado inahitaji kuhakikiwa. | draft | Flag: Drafter: Plural pairs: '1' form uses class 9 'Rekodi 1 ... inahitaji', other uses 'zinahitaji'. 'Rekodi' is invariable so both read correctly. |
| `officerHome.outstanding_other` | {{count}} records still need verifying. | Rekodi {{count}} bado zinahitaji kuhakikiwa. | draft | Keep {{count}} |
| `officerHome.people` | People | Watu | draft |  |
| `officerHome.register` | Register a farmer | Sajili mkulima | draft |  |
| `officerHome.requests` | Equipment requests | Maombi ya vifaa | draft |  |
| `officerHome.title` | Your villages | Vijiji vyako | draft |  |
| `officerHome.villageOutstanding_one` | 1 record here still needs verifying | Rekodi 1 hapa bado inahitaji kuhakikiwa | draft |  |
| `officerHome.villageOutstanding_other` | {{count}} records here still need verifying | Rekodi {{count}} hapa bado zinahitaji kuhakikiwa | draft | Keep {{count}} |
| `officerHome.villages` | Assigned villages | Vijiji ulivyopangiwa | draft | Flag: Drafter: 'Vijiji ulivyopangiwa' (villages you were assigned to). |
| `opportunityStatus.accepted` | Accepted | Imekubaliwa | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunityStatus.declined` | Declined | Mnunuzi amekataa | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunityStatus.lapsed` | Lapsed | Imeisha muda wake | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. Flag: Drafter: 'Imeisha muda wake' glossary UNVERIFIED. |
| `opportunityStatus.proposed` | Proposed | Imependekezwa | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunityStatus.shared` | Shared | Imeshirikishwa | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. Flag: Drafter: 'Imeshirikishwa' (shared with buyer) composed; not in glossary. |
| `people.allVerifications` | All | Wote | draft |  |
| `people.colName` | Name | Jina | draft |  |
| `people.colPhone` | Phone | Simu | draft |  |
| `people.colProvenance` | Provenance | Chanzo cha taarifa | draft | Flag: Drafter: 'Chanzo cha taarifa' unverified per glossary. |
| `people.colVillage` | Village | Kijiji | draft |  |
| `people.filterVerification` | Verification | Uhakiki | draft | Flag: Drafter: 'Uhakiki' (noun for verification) derived from 'hakiki'. |
| `people.noneDetail` | Try a different name or clear the verification filter. | Jaribu jina lingine au ondoa kichujio cha uhakiki. | draft |  |
| `people.noneTitle` | No people match | Hakuna watu wanaolingana | draft |  |
| `people.search` | Search by name | Tafuta kwa jina | draft |  |
| `people.searchPlaceholder` | Given or family name | Jina la kwanza au la ukoo | draft | Flag: Drafter: 'Jina la kwanza au la ukoo' for 'Given or family name'. |
| `people.title` | People | Watu | draft |  |
| `person.actual` | Actual | Halisi | draft |  |
| `person.allVerified` | Every record here is verified | Kila rekodi hapa imehakikiwa | draft |  |
| `person.cycles` | Crop cycles | Misimu ya mazao | draft | Flag: Drafter: 'Misimu ya mazao' plural of composed term 'msimu wa zao'. |
| `person.expected` | Expected | Inatarajiwa | draft |  |
| `person.farms` | Farms | Mashamba | draft |  |
| `person.harvests` | Harvest figures | Kiasi cha mavuno | draft |  |
| `person.households` | Households | Kaya | draft |  |
| `person.members` | Members | Wanakaya | draft | Flag: Drafter: 'Wanakaya' (household members, glossary 'mwanakaya'). |
| `person.noFarms` | No farms recorded yet | Bado hakuna shamba lililorekodiwa | draft |  |
| `person.noFarmsDetail` | Farms appear here once one is registered. | Mashamba yataonekana hapa baada ya shamba kusajiliwa. | draft |  |
| `person.noHouseholds` | No household recorded | Hakuna kaya iliyorekodiwa | draft |  |
| `person.notFoundDetail` | This record may not exist, or you may not have access to it. | Rekodi hii huenda haipo, au huenda huna ruhusa ya kuiona. | draft | Flag: Changed after the cross-check; unreviewed. |
| `person.notFoundTitle` | Person not found | Mtu hajapatikana | draft |  |
| `person.plantedArea` | Planted area | Eneo lililopandwa | draft | This is planted area ACROSS CYCLES, never "land area": intercropping means it can exceed the village’s hectares. Flag: Same open question as register.cycleArea: on this screen the figure may be summed across cycles. Drafter: Glossary across-cycles phrase (UNVERIFIED); shortened form used in the edit field for the same concept. |
| `person.plots` | Plots | Vipande vya shamba | draft |  |
| `person.superseded` | Superseded | Imebadilishwa na mpya | draft | Flag: Drafter: 'Imebadilishwa na mpya' (replaced by a newer one); shown in parentheses after a harvest figure. No glossary term; could be confused with 'edited'. |
| `person.trees` | Trees | Miti | draft |  |
| `person.units` | Units | Vitengo | draft |  |
| `person.unverifiedCount_one` | {{count}} record still needs verifying | Rekodi {{count}} bado haijahakikiwa | draft | Keep {{count}} |
| `person.unverifiedCount_other` | {{count}} records still need verifying | Rekodi {{count}} bado hazijahakikiwa | draft | Keep {{count}} Flag: Drafter: Plural agreement 'rekodi ... hazijahakikiwa' vs singular 'haijahakikiwa' (rekodi class 9/10); 'haijahakikiwa' UNVERIFIED in glossary. Chose 'bado haijahakikiwa' over 'bado inahitaji kuhakikiwa' for brevity; reviewer decide. |
| `person.verified` | Verified | Imehakikiwa | draft |  |
| `person.verify` | Verify | Hakiki | draft |  |
| `person.verifying` | Verifying… | Inahakiki… | draft |  |
| `person.window` | Harvest window | Kipindi cha kuvuna | draft | Flag: Drafter: 'Kipindi cha kuvuna' UNVERIFIED composition. Date-field labels use 'Mwanzo/Mwisho wa kipindi cha kuvuna' (my construction). |
| `plotDocuments.add` | Add documents | Ongeza hati | draft |  |
| `plotDocuments.adding` | Filing… | Inahifadhi… | draft |  |
| `plotDocuments.none` | No title documents filed for this plot. | Hakuna hati ya ardhi iliyohifadhiwa kwa kipande hiki. | draft |  |
| `plotDocuments.staffOnly` | Staff only. The farmer does not see these. | Kwa watumishi tu. Mkulima hazioni. | draft | Flag: Drafter: 'watumishi' for staff, unverified. |
| `plotDocuments.title_one` | Title documents ({{count}}) | Hati za ardhi ({{count}}) | draft | Keep {{count}} Flag: Drafter: shorter 'hati za ardhi' (land documents), unverified; see register.documents. |
| `plotDocuments.title_other` | Title documents ({{count}}) | Hati za ardhi ({{count}}) | draft | Keep {{count}} |
| `provenance.capturedAt` | Captured | Ilikusanywa | draft | Provenance wording: where a record came from, and who verified it. Flag: Drafter: 'Ilikusanywa' (was collected) for 'Captured'; alternative 'Ilirekodiwa' (was recorded). Shown next to a date and 'na' + name. |
| `provenance.capturedBy` | by | na | draft | Provenance wording: where a record came from, and who verified it. Flag: Drafter: Bare 'na' (by) reads oddly alone but works between a date and a name. |
| `provenance.confidenceLabel` | Confidence | Kiwango cha uhakika | draft | Provenance wording: where a record came from, and who verified it. Flag: Drafter: 'Kiwango cha uhakika' unverified per glossary. |
| `redeem.amount` | Incentive to hand over | Motisha ya kukabidhi | draft |  |
| `redeem.another` | Redeem another | Kabidhi nyingine | draft | Flag: Drafter: 'Redeem another' shortened to 'Vocha nyingine' (another voucher) for button length; reviewer may want a full verb. |
| `redeem.cameraUnavailable` | The camera is not available on this device. Type the code instead. | Kamera haipatikani kwenye kifaa hiki. Andika namba ya vocha badala yake. | draft |  |
| `redeem.cannotRedeem` | You cannot redeem this voucher | Huwezi kukabidhi vocha hii | draft |  |
| `redeem.code` | Voucher code | Namba ya vocha | draft |  |
| `redeem.confirm` | Confirm and hand over {{amount}} | Thibitisha na ukabidhi {{amount}} | draft | Keep {{amount}} Flag: Drafter: Uses 'Thibitisha' (Confirm) per glossary; fine since verify=hakiki is a different concept. |
| `redeem.confirming` | Redeeming… | Inakabidhi… | draft |  |
| `redeem.done` | Redeemed | Imekabidhiwa | draft |  |
| `redeem.doneDetail` | {{amount}} handed over. This voucher cannot be used again. | {{amount}} imekabidhiwa. Vocha hii haiwezi kutumika tena. | draft | Keep {{amount}} |
| `redeem.expires` | Valid until {{date}} | Inatumika hadi {{date}} | draft | Keep {{date}} |
| `redeem.head` | Head of household | Mkuu wa kaya | draft |  |
| `redeem.household` | Household | Kaya | draft |  |
| `redeem.idType` | ID document you checked | Kitambulisho ulichokagua | draft |  |
| `redeem.idTypePlaceholder` | Choose the document | Chagua kitambulisho | draft |  |
| `redeem.intro` | Scan the farmer's QR code, or type the code printed under it. | Skani msimbo wa QR wa mkulima, au andika namba iliyochapishwa chini yake. | draft |  |
| `redeem.lookingUp` | Looking up… | Inatafuta… | draft |  |
| `redeem.lookUp` | Look up | Tafuta | draft |  |
| `redeem.nameConfirm` | The name on the ID matches the household above | Jina kwenye kitambulisho linalingana na kaya iliyo hapo juu | draft | Flag: Drafter: Checkbox text; 'linalingana na' (matches) composed. |
| `redeem.needsOps` | Held for audit: ops or admin must redeem this voucher in person. | Imeshikiliwa kwa ukaguzi: mfanyakazi wa uendeshaji au msimamizi lazima akabidhi vocha hii ana kwa ana. | draft | Flag: Drafter: 'ops or admin' -> 'timu ya uendeshaji au msimamizi' (composed); 'Held for audit' UNVERIFIED in glossary. |
| `redeem.notAVoucher` | That is not a Ruaha voucher code. | Hiyo si namba ya vocha ya Ruaha. | draft |  |
| `redeem.notFound` | No voucher with this code in your villages. | Hakuna vocha yenye namba hii katika vijiji vyako. | draft |  |
| `redeem.respondent` | Answered by | Imejibiwa na | draft | Flag: Drafter: 'Answered by' -> 'Imejibiwa na' (class 9 concord for a label; may need 'Alijibu' if it names a person). |
| `redeem.scan` | Scan QR code | Skani msimbo wa QR | draft |  |
| `redeem.stopScan` | Stop camera | Zima kamera | draft |  |
| `redeem.survey` | Survey | Dodoso | draft |  |
| `redeem.title` | Redeem a voucher | Kabidhi motisha ya vocha | draft | Flag: Drafter: 'Redeem a voucher' has no Tanzanian term. Rendered 'Kabidhi motisha ya vocha' (hand over the voucher's incentive); glossary options kabidhi/chukua are UNVERIFIED. Also drives redeem.confirm, done, doneDetail, cannotRedeem. |
| `register.chooseCrop` | Choose a crop | Chagua zao | draft |  |
| `register.confidence` | Confidence | Kiwango cha uhakika | draft | Confidence level recorded with a figure. Low / medium / high. Flag: Drafter: 'Kiwango cha uhakika' UNVERIFIED (glossary). Low/medium/high option strings are in another batch. |
| `register.crop` | Crop | Zao | draft |  |
| `register.crops` | Crops grown on this plot | Mazao yanayolimwa kwenye kipande hiki | draft |  |
| `register.cropsHint` | Tick every crop. Each one gets its own measure, harvest window and expected harvest. | Weka alama kwenye kila zao. Kila zao lina kipimo chake, kipindi chake cha mavuno na mavuno yake yanayotarajiwa. | draft |  |
| `register.cropsRequired` | Choose at least one crop. | Chagua angalau zao moja. | draft |  |
| `register.cycleArea` | Planted area (ha) | Eneo lililopandwa (ha) | draft | This is planted area ACROSS CYCLES, never "land area": intercropping means it can exceed the village’s hectares. Flag: Wording changed after the cross-check to the short, English-faithful label. Open question: this field is planted area for one crop cycle, and business-rules says it is never "land area". Reviewer to confirm the label and whether "across cycles" belongs here. Drafter: Note requires 'planted area across cycles'; used the glossary full phrase (UNVERIFIED). But this is a single-cycle form field, so 'katika mazao yote' (across all crops) may read oddly; reviewer decide. officerEdit.fields.cycle_area_ha (no note) uses the short 'Eneo lililopandwa (ha)'. Cross-check (rule): The locked phrase says 'total area planted across all the crops' but this field records ONE crop cycle's area; an officer may enter a sum over all crops, and 'across crops' is not the same as the note's 'across cycles'. Also 49 characters for a 17-character English label. Needs native decision, not a silent change. |
| `register.documents` | Title documents | Hati za umiliki wa ardhi | draft | Flag: Drafter: 'hati za umiliki wa ardhi' (title deed = 'hati miliki' in common Tanzanian usage) is NOT in the glossary: unverified. Native reviewer. |
| `register.documentsAdd` | Add photos or files | Ongeza picha au faili | draft |  |
| `register.documentsFailed` | The farmer is registered, but these documents were not filed: | Mkulima amesajiliwa, lakini hati hizi hazijahifadhiwa: | draft |  |
| `register.documentsFiled_one` | {{count}} title document filed with the plot. | Hati {{count}} ya ardhi imehifadhiwa pamoja na kipande. | draft | Keep {{count}} |
| `register.documentsFiled_other` | {{count}} title documents filed with the plot. | Hati {{count}} za ardhi zimehifadhiwa pamoja na kipande. | draft | Keep {{count}} |
| `register.documentsHint` | Photos or PDFs of the title deed or other land papers, so whoever verifies this plot can check it against them. Staff only: the farmer does not see them. | Picha au PDF za hati miliki au karatasi nyingine za ardhi, ili anayehakiki kipande hiki aweze kukilinganisha nazo. Ni kwa watumishi tu: mkulima hazioni. | draft | Flag: Drafter: 'hati miliki' and 'watumishi' (staff) composed, unverified. |
| `register.documentsRemove` | Remove {{name}} | Ondoa {{name}} | draft | Keep {{name}} |
| `register.documentsRemoveShort` | Remove | Ondoa | draft |  |
| `register.documentsRetry` | Try filing them again | Jaribu kuzihifadhi tena | draft |  |
| `register.documentsUploading` | Filing the title documents… | Inahifadhi hati za ardhi… | draft |  |
| `register.familyName` | Family name | Jina la ukoo | draft |  |
| `register.farmLabel` | Farm name | Jina la shamba | draft |  |
| `register.givenName` | First name | Jina la kwanza | draft |  |
| `register.gpsDenied` | Location access was declined. Enter the coordinates yourself, or | Ruhusa ya kupata mahali ilikataliwa. Andika viwianishi mwenyewe, au | draft | Flag: Drafter: Sentence ends with 'au' because a 'Jaribu tena' button follows; 'viwianishi' (coordinates) is my word choice, not in glossary. Same for gpsUnsupported and gpsError. |
| `register.gpsDetecting` | Finding this farm's location… | Inatafuta mahali lilipo shamba hili… | draft |  |
| `register.gpsError` | Couldn't get a location just now. Enter the coordinates yourself, or | Imeshindwa kupata mahali kwa sasa. Andika viwianishi mwenyewe, au | draft |  |
| `register.gpsNote` | The farm's position is required. It comes from this phone's GPS; if GPS is not available, type it in. | Mahali pa shamba ni lazima. Hupatikana kupitia GPS ya simu hii; ikiwa GPS haipatikani, paandike mwenyewe. | draft | Flag: Drafter: 'Mahali pa shamba' (location of farm) used for 'position'; class-16 agreement 'paandike' is natural but check. |
| `register.gpsRetry` | Try again | Jaribu tena | draft |  |
| `register.gpsUnsupported` | This browser can't detect location. Enter the coordinates yourself, or | Kivinjari hiki hakiwezi kutambua mahali. Andika viwianishi mwenyewe, au | draft |  |
| `register.gpsUseCurrent` | Use my current location | Tumia mahali nilipo sasa | draft | Flag: Drafter: 'Tumia mahali nilipo sasa' (use the place I am now) is composed; 'mahali' for location follows gpsNote. Not verified for Tanzanian usage. |
| `register.groupsComplete` | {{done}} of {{total}} groups complete · saved on this phone only | Makundi {{done}} kati ya {{total}} yamekamilika · yamehifadhiwa kwenye simu hii pekee | draft | Keep {{done}} {{total}} Flag: Drafter: 'Makundi' for 'groups' (form sections); 'kwenye simu hii pekee' composed. |
| `register.harvestEnd` | Harvest window ends | Mwisho wa kipindi cha kuvuna | draft |  |
| `register.harvestKg` | Expected harvest (kg) | Mavuno yanayotarajiwa (kg) | draft |  |
| `register.harvestStart` | Harvest window starts | Mwanzo wa kipindi cha kuvuna | draft |  |
| `register.householdLabel` | Household name | Jina la kaya | draft |  |
| `register.intro` | One page, one submit. Everything below is created together. | Ukurasa mmoja, uwasilishaji mmoja. Kila kitu kilicho hapa chini kinaundwa pamoja. | draft |  |
| `register.isHead` | This person heads the household | Mtu huyu ni mkuu wa kaya | draft |  |
| `register.latitude` | Latitude | Latitudo | draft | Flag: Drafter: 'Latitudo'/'Longitudo' are standard loanwords I chose; no glossary entry. Not verified for Tanzanian usage. |
| `register.latitudeRange` | Latitude runs from -90 to 90. | Latitudo ni kati ya -90 na 90. | draft |  |
| `register.longitude` | Longitude | Longitudo | draft |  |
| `register.longitudeRange` | Longitude runs from -180 to 180. | Longitudo ni kati ya -180 na 180. | draft |  |
| `register.measuredBy` | {{crop}} is measured this way | {{crop}} hupimwa kwa njia hii | draft | Keep {{crop}} Flag: Drafter: Hint under area/trees/units meaning 'this crop is counted by this measure'. Meaning is my reading of the screen; wording 'hupimwa kwa njia hii' may be unclear. |
| `register.notANumber` | Enter a number. | Andika namba. | draft |  |
| `register.notNegative` | This cannot be negative. | Hii haiwezi kuwa chini ya sifuri. | draft |  |
| `register.noVillage` | No village to register into | Hakuna kijiji cha kusajili | draft |  |
| `register.noVillageDetail` | Registration needs a village assignment. Ask your programme manager. | Ili kusajili, unahitaji kupangiwa kijiji. Muulize meneja wako wa mradi. | draft | Flag: Drafter: 'Usajili unahitaji ugawiwe kijiji' (assigned a village) is composed; 'meneja wa programu' not in glossary. |
| `register.phone` | Phone | Namba ya simu | draft |  |
| `register.phoneHint` | Include the country code, for example +255700000101. | Weka msimbo wa nchi, kwa mfano +255700000101. | draft |  |
| `register.plantedAcrossNote` | Planted area is entered for each crop below. Across several crops it can add up to more than the plot's area. | Eneo lililopandwa linaandikwa kwa kila zao hapa chini. Kwa mazao kadhaa, jumla yake inaweza kuzidi eneo la kipande. | draft | Flag: Drafter: composed from the glossary's 'eneo lililopandwa' and 'kipande'. Carries the labelling rule that planted area across cycles can exceed land area. Native reviewer. |
| `register.plantedOn` | Planted on | Tarehe ya kupanda | draft | Flag: Drafter: Used 'Tarehe ya kupanda' rather than 'Ilipopandwa'; glossary notes sources disagree on kupanda vs kuotesha. |
| `register.plotArea` | Plot area (ha) | Eneo la kipande cha shamba (ha) | draft |  |
| `register.plotLabel` | Plot name | Jina la kipande cha shamba | draft |  |
| `register.provenanceNote` | Recorded as field-verified, captured by you. | Imehakikiwa shambani, imeingizwa na wewe. | draft | Provenance wording: where a record came from, and who verified it. Flag: Drafter: 'Field-verified' rendered 'imehakikiwa shambani' (UNVERIFIED glossary); 'imeingizwa na wewe' for 'captured by you'. Shortened 30 Sep 2026 to fit one line on a phone; the former 'This is not a choice' clause was dropped. |
| `register.registerAnother` | Register another | Sajili mwingine | draft |  |
| `register.required` | This is required. | Hii ni lazima. | draft | Flag: Drafter: Used 'Sehemu hii ni lazima.' (glossary gives 'ni lazima'; sentence form composed). |
| `register.roundedNote` | This will be stored as {{value}}. | Itahifadhiwa kama {{value}}. | draft | Keep {{value}} |
| `register.sections.cycle` | Crops | Mazao | draft | Flag: Drafter: 'Mazao' (plural of zao, glossary-attested for crop). Section became a list of crops on 5 Oct 2026; was 'Msimu wa zao'. |
| `register.sections.farm` | Farm | Shamba | draft |  |
| `register.sections.harvest` | Expected harvest | Mavuno yanayotarajiwa | draft | Flag: Drafter: 'Mavuno yanayotarajiwa' UNVERIFIED (glossary). person.expected uses 'Inatarajiwa' for the short column label; person.actual 'Halisi' unchecked. |
| `register.sections.household` | Household | Kaya | draft |  |
| `register.sections.person` | Person | Mtu | draft |  |
| `register.sections.plot` | Plot | Kipande | draft | Flag: Drafter: Plot = 'kipande cha shamba' (UNVERIFIED, glossary). Used in full everywhere; long for a wizard rail label, may want 'Kipande'. |
| `register.submit` | Register | Sajili | draft | Flag: Drafter: 'Sajili' as imperative for registering someone else (glossary). registerAnother 'Sajili mwingine' assumes 'mkulima' understood. |
| `register.submitting` | Registering… | Inasajili… | draft |  |
| `register.successDetail` | The records were created together. | Rekodi zimeundwa pamoja. | draft |  |
| `register.successTitle` | Registered | Imesajiliwa | draft |  |
| `register.title` | Register a farmer | Sajili mkulima | draft |  |
| `register.tooLarge` | This number is too large for the field. | Namba hii ni kubwa mno kwa sehemu hii. | draft |  |
| `register.treeCount` | Number of trees | Idadi ya miti | draft |  |
| `register.unitCount` | Number of units | Idadi ya vitengo | draft | Flag: Drafter: 'Vitengo' for 'units' (generic counted units); not in glossary. Also person.units and officerEdit.fields.unit_count. |
| `register.viewPerson` | Open the record | Fungua rekodi | draft |  |
| `register.wholeNumber` | Enter a whole number. | Andika namba nzima. | draft |  |
| `register.windowBackwards` | The harvest window must end on or after it starts. | Kipindi cha kuvuna lazima kiishe siku kinapoanza au baada yake. | draft | Flag: Drafter: Long composed validation sentence; check grammar and naturalness. |
| `requests.action.submit` | Send this request | Tuma ombi hili | draft |  |
| `requests.action.withdraw` | Withdraw | Ondoa ombi | draft | Flag: Cross-check: "Ondoa ombi" may read as deleting the request. Withdraw means the farmer takes it back and the record remains. Drafter: 'Withdraw' = 'Ondoa ombi' (glossary 'remove'); may read as delete rather than withdraw. Cross-check (meaning): "Ondoa ombi" back-translates as "Remove request", which reads like deleting; English Withdraw means the farmer takes the request back but the record remains. |
| `requests.assumptions` | Assumptions | Dhana | draft |  |
| `requests.decision` | Decision | Uamuzi | draft |  |
| `requests.draftNote` | This request has not been sent yet. Nobody is reviewing it until you send it. | Ombi hili bado halijatumwa. Hakuna anayelipitia hadi utakapolituma. | draft |  |
| `requests.frozen` | A submitted request cannot be changed. Ask your field officer if something needs correcting. | Ombi lililowasilishwa haliwezi kubadilishwa. Mwombe afisa wako wa uwandani ikiwa kuna kitu cha kurekebisha. | draft |  |
| `requests.noEstimate` | No estimate stored for this request | Hakuna makadirio yaliyohifadhiwa kwa ombi hili | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. |
| `requests.noEstimateDetail` | This equipment has no rated power recorded, so no estimate could be calculated. | Kifaa hiki hakina nguvu iliyoandikwa iliyorekodiwa, kwa hiyo makadirio hayakuweza kukokotolewa. | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. |
| `requests.noneDetail` | Requests you submit will appear here. | Maombi utakayowasilisha yataonekana hapa. | draft |  |
| `requests.noneTitle` | No requests yet | Bado hakuna maombi | draft |  |
| `requests.notFoundDetail` | It may not exist, or you may not have access to it. | Huenda halipo, au huna ruhusa ya kulifikia. | draft |  |
| `requests.notFoundTitle` | Request not found | Ombi halikupatikana | draft |  |
| `requests.storedEstimate` | Stored estimate | Makadirio yaliyohifadhiwa | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. |
| `requests.submittedOn` | Submitted | Imewasilishwa | draft |  |
| `requests.title` | My requests | Maombi yangu | draft |  |
| `requestStatus.approved` | Approved | Imeidhinishwa | draft |  |
| `requestStatus.draft` | Draft | Rasimu | draft |  |
| `requestStatus.rejected` | Rejected | Imekataliwa | draft |  |
| `requestStatus.submitted` | Submitted | Imewasilishwa | draft |  |
| `requestStatus.under_review` | Under review | Inapitiwa | draft | Flag: Drafter: 'Inapitiwa' glossary UNVERIFIED. |
| `requestStatus.withdrawn` | Withdrawn | Imeondolewa | draft | Flag: Cross-check: "Imeondolewa" may read as removed or deleted, and does not say who withdrew it. Drafter: 'Imeondolewa' (removed) for Withdrawn; unverified, could read as deleted. Cross-check (meaning): "Imeondolewa" back-translates as "Removed", which does not say who withdrew it and may read as deleted or taken away by ops. |
| `resources.acquisition.buy` | To buy | Kununua | draft | Provenance wording: where a record came from, and who verified it. |
| `resources.acquisition.rent` | To rent | Kukodi | draft | Provenance wording: where a record came from, and who verified it. |
| `resources.askAtOffice` | Ask at the Ruaha office | Uliza ofisi ya Ruaha | draft | Provenance wording: where a record came from, and who verified it. |
| `resources.kind.equipment` | Equipment | Vifaa | draft | Provenance wording: where a record came from, and who verified it. |
| `resources.kind.loan` | Loans | Mikopo | draft | Provenance wording: where a record came from, and who verified it. Flag: Drafter: 'Mikopo' (loans) attested only in a Vodacom FAQ in the glossary: unverified. |
| `resources.kindLabel` | Kind of resource | Aina ya rasilimali | draft | Provenance wording: where a record came from, and who verified it. |
| `resources.loanNote` | A loan listing is not an offer and not an approval. Amounts are indicative. Ask at the Ruaha office whether you are eligible. | Mkopo ulioorodheshwa si ahadi wala idhini ya mkopo. Viasi ni elekezi. Uliza ofisi ya Ruaha kama unastahili. | draft | Provenance wording: where a record came from, and who verified it. Flag: Drafter: 'si ahadi wala idhini ya mkopo' (not a promise nor approval of a loan) composed to carry 'not an offer'. Labelling rule: must not read as an offer. Native reviewer. |
| `resources.noLoansDetail` | Loan listings appear here once the programme adds them. | Mikopo itaonekana hapa programu itakapoiongeza. | draft | Provenance wording: where a record came from, and who verified it. |
| `resources.noLoansTitle` | No loans listed yet | Bado hakuna mikopo iliyoorodheshwa | draft | Provenance wording: where a record came from, and who verified it. |
| `resources.offered.buy` | Buy | Kununua | draft | Provenance wording: where a record came from, and who verified it. Flag: Drafter: 'Kununua' (to buy) composed: unverified. |
| `resources.offered.rent` | Rent | Kukodi | draft | Provenance wording: where a record came from, and who verified it. Flag: Drafter: 'Kukodi' (to rent) not in glossary: unverified. |
| `resources.rentPerDay` | Rent per day | Kodi kwa siku | draft | Provenance wording: where a record came from, and who verified it. |
| `resources.title` | Equipment and loans | Vifaa na mikopo | draft | Provenance wording: where a record came from, and who verified it. |
| `role.admin` | Administrator | Msimamizi | draft | Flag: Drafter: 'Msimamizi' unverified role label. |
| `role.farmer` | Farmer | Mkulima | draft |  |
| `role.field_officer` | Field officer | Afisa wa uwandani | draft | Flag: Drafter: 'Afisa wa uwandani' composed, unverified; avoids 'Afisa Ugani'. |
| `role.ops` | Operations | Uendeshaji | draft | Flag: Drafter: 'Uendeshaji' unverified role label. |
| `selectRole.continue` | Continue | Endelea | draft |  |
| `selectRole.detail` | You hold more than one role. Pick the one you want to work in. | Una majukumu zaidi ya moja. Chagua unalotaka kufanyia kazi. | draft |  |
| `selectRole.noneDetail` | Your access may have changed. Ask your programme manager. | Ufikiaji wako huenda umebadilika. Muulize meneja wa mradi. | draft | Flag: Drafter: 'meneja wa programu' for 'programme manager' is not in glossary; also used in noAccess.detail and officerHome.noVillagesDetail. |
| `selectRole.noneTitle` | No roles to choose from | Hakuna majukumu ya kuchagua | draft |  |
| `selectRole.title` | Choose a role | Chagua jukumu | draft | Flag: Drafter: 'Jukumu' used for 'role'; not in glossary. |
| `selectRole.wholeProject` | Whole project | Mradi mzima | draft |  |
| `setPassword.confirm` | Type it again | Liandike tena | draft |  |
| `setPassword.detail` | You signed in with a temporary password from your officer. Choose a new one that only you know. | Uliingia kwa neno la siri la muda kutoka kwa afisa wako. Chagua neno la siri jipya ambalo unalijua wewe tu. | draft | Flag: Drafter: 'wewe tu' used for emphasis (only you know it); acceptable per tone rule but reviewer may prefer 'ambalo ni wewe pekee unalijua'. |
| `setPassword.mismatch` | The two passwords are not the same. | Maneno mawili ya siri hayafanani. | draft |  |
| `setPassword.password` | New password | Neno la siri jipya | draft |  |
| `setPassword.required` | Enter a new password. | Weka neno la siri jipya. | draft |  |
| `setPassword.submit` | Save password | Hifadhi neno la siri | draft |  |
| `setPassword.submitting` | Saving… | Inahifadhi… | draft |  |
| `setPassword.title` | Choose your own password | Chagua neno lako la siri mwenyewe | draft | Flag: Changed after the cross-check; unreviewed. |
| `setPassword.unexpected` | Something went wrong saving your password. | Kuna tatizo katika kuhifadhi neno lako la siri. | draft |  |
| `source.farmer_reported` | Farmer reported | Imeripotiwa na mkulima | draft | Provenance wording: where a record came from, and who verified it. |
| `source.field_verified` | Field verified | Imehakikiwa shambani | draft | Provenance wording: where a record came from, and who verified it. |
| `source.model_estimated` | Estimated | Imekadiriwa | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. |
| `source.sensor_derived` | Sensor measured | Imepimwa na kihisi | draft | Provenance wording: where a record came from, and who verified it. Flag: Drafter: 'Imepimwa na kihisi' composed; 'kihisi' (sensor) not in glossary. |
| `source.transaction_derived` | From a transaction | Kutoka kwenye muamala | draft | Provenance wording: where a record came from, and who verified it. Flag: Drafter: 'Kutoka kwenye muamala' composed; 'muamala' (transaction) used by Airtel per glossary but not a glossary row. |
| `surveys.answered` | Answered | Limejibiwa | draft | Flag: Drafter: 'Limejibiwa' (class 5 agreement with dodoso), not attested; 'Answered' may also label a card on a survey. Same for 'Halipatikani' (Not available). |
| `surveys.badge_one` | {{count}} new survey | dodoso jipya {{count}} | draft | Keep {{count}} Flag: Drafter: 'dodoso jipya {{count}}' / 'madodoso mapya {{count}}': agreement chosen by me. |
| `surveys.badge_other` | {{count}} new surveys | madodoso mapya {{count}} | draft | Keep {{count}} |
| `surveys.closes` | Closes {{date}} | Linafungwa {{date}} | draft | Keep {{date}} |
| `surveys.emptyDetail` | New surveys will appear here. | Madodoso mapya yataonekana hapa. | draft |  |
| `surveys.emptyTitle` | No surveys right now | Hakuna madodoso kwa sasa | draft |  |
| `surveys.incentive` | Incentive | Motisha | draft |  |
| `surveys.incentiveNote` | Paid in cash at the Ruaha office | Hukabidhiwa kwa fedha taslimu katika ofisi ya Ruaha | draft | Flag: Drafter: 'Paid in cash' -> 'Hukabidhiwa kwa fedha taslimu' (handed over in cash) deliberately, to avoid payment vocabulary. |
| `surveys.intro` | Answer a survey and your household receives a fixed cash incentive, once per survey, paid at the Ruaha office. | Jibu dodoso na kaya yako itapokea motisha ya kiasi maalum cha fedha taslimu, mara moja kwa kila dodoso, inayokabidhiwa katika ofisi ya Ruaha. | draft | Flag: Drafter: Incentive expressed as 'motisha ya kiasi maalum cha fedha taslimu' (glossary; 'kiasi maalum' UNVERIFIED). Avoided 'kulipwa' entirely; 'kukabidhiwa' (handed over) used for 'paid at the office'. |
| `surveys.new` | New | Jipya | draft | Flag: Drafter: 'New' is the badge on a survey (dodoso, class 5): 'Jipya' agrees with dodoso; if used for other nouns it would need to change. |
| `surveys.no` | No | Hapana | draft |  |
| `surveys.notAvailable` | Not available | Halipatikani | draft |  |
| `surveys.notFound` | This survey is not available. | Dodoso hili halipatikani. | draft |  |
| `surveys.onceNote` | Your household can answer this survey once. Check your answers before you submit. | Kaya yako inaweza kujibu dodoso hili mara moja. Kagua majibu yako kabla ya kuwasilisha. | draft |  |
| `surveys.open` | Answer survey | Jibu dodoso | draft |  |
| `surveys.optional` | Optional | Si lazima | draft | Flag: Drafter: 'Si lazima' UNVERIFIED in glossary; 'hiari' alternative. |
| `surveys.questionCount_one` | {{count}} question | swali {{count}} | draft | Keep {{count}} Flag: Drafter: Number placed after noun ('swali {{count}}') to work for count=1 and !=1; reviewer may prefer 'Swali {{count}}'. |
| `surveys.questionCount_other` | {{count}} questions | maswali {{count}} | draft | Keep {{count}} |
| `surveys.required` | Required | Lazima | draft |  |
| `surveys.submit` | Submit answers | Wasilisha majibu | draft |  |
| `surveys.submitting` | Submitting… | Inawasilisha… | draft |  |
| `surveys.title` | Surveys | Madodoso | draft | Flag: Drafter: 'Surveys' as 'Madodoso' (dodoso = questionnaire, glossary). 'Utafiti' considered but means research. |
| `surveys.view` | View voucher | Angalia vocha | draft |  |
| `surveys.yes` | Yes | Ndiyo | draft |  |
| `surveyStatus.closed` | Closed | Limefungwa | draft | Flag: Changed after the cross-check; unreviewed. |
| `surveyStatus.draft` | Draft | Rasimu | draft |  |
| `surveyStatus.live` | Live | Linaendelea | draft | Flag: Drafter: 'Live' for a survey: 'Inaendelea' (ongoing) is my choice; not in glossary. Reviewer to confirm. |
| `table.emptyDetail` | No records match. | Hakuna rekodi zinazolingana. | draft |  |
| `table.emptyTitle` | Nothing to show | Hakuna cha kuonyesha | draft |  |
| `tour.back` | Back | Rudi nyuma | draft |  |
| `tour.close` | Done | Maliza | draft | Flag: Drafter: 'Maliza' [UNVERIFIED] for Done; alternative 'Funga'. |
| `tour.farmer.chapter.equipment` | Equipment | Vifaa | draft | Flag: Drafted after the main cross-check, for tour strings added late (chapter menu). Not back-translated. Unreviewed. |
| `tour.farmer.chapter.home` | Your home screen | Skrini yako ya nyumbani | draft | Flag: Drafted after the main cross-check, for tour strings added late (chapter menu). Not back-translated. Unreviewed. |
| `tour.farmer.chapter.myFarm` | My farm | Shamba langu | draft | Flag: Drafted after the main cross-check, for tour strings added late (chapter menu). Not back-translated. Unreviewed. |
| `tour.farmer.chapter.next` | What happens next | Kinachofuata | draft | Flag: Drafted after the main cross-check, for tour strings added late (chapter menu). Not back-translated. Unreviewed. |
| `tour.farmer.chapter.opportunities` | Opportunities | Fursa | draft | Flag: Drafted after the main cross-check, for tour strings added late (chapter menu). Not back-translated. Unreviewed. |
| `tour.farmer.chapter.requests` | My requests | Maombi yangu | draft | Flag: Drafted after the main cross-check, for tour strings added late (chapter menu). Not back-translated. Unreviewed. |
| `tour.farmer.chapter.surveys` | Surveys and vouchers | Madodoso na vocha | draft | Flag: Drafted after the main cross-check, for tour strings added late (chapter menu). Not back-translated. Unreviewed. |
| `tour.farmer.chapter.voucher` | Your voucher | Vocha yako | draft |  |
| `tour.farmer.chapter.welcome` | Welcome | Karibu | draft | Flag: Drafted after the main cross-check, for tour strings added late (chapter menu). Not back-translated. Unreviewed. |
| `tour.farmer.equipmentEstimateBody` | Rated power times how many gives the estimated peak power. Times hours per day gives kWh a day, and times days per week gives kWh a week. It comes from your figures, so it stays an estimate. | Nguvu iliyoandikwa mara idadi inatoa kilele kilichokadiriwa. Mara saa kwa siku inatoa kWh kwa siku, na mara siku kwa wiki inatoa kWh kwa wiki. Yanatokana na namba zako, kwa hiyo yanabaki kuwa makadirio. | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. Flag: Drafter: Uses on-screen label 'kilele kilichokadiriwa' for Estimated peak (glossary [UNVERIFIED]); 'mara' for multiplication in prose. |
| `tour.farmer.equipmentEstimateTitle` | How the estimate is worked out | Makadirio yanavyokokotolewa | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. |
| `tour.farmer.equipmentListBody` | Powered equipment the programme offers, to rent or to buy, with its rated power and indicative prices — and, under Loans, loan listings to ask about at the office. A price is a guide, not a quotation, and nothing is ordered from this list. | Vifaa vya umeme ambavyo programu inatoa, vya kukodi au kununua, pamoja na nguvu iliyoandikwa na bei elekezi, na chini ya Mikopo, orodha ya mikopo ya kuuliza ofisini. Bei ni mwongozo, si nukuu ya bei, na hakuna kinachoagizwa kutoka orodha hii. | draft |  |
| `tour.farmer.equipmentListTitle` | Equipment and loans | Vifaa na mikopo | draft |  |
| `tour.farmer.equipmentOpenBody` | Tap any machine to see it in full and ask for it. Next opens the first one for you. | Gusa kifaa chochote ukione chote na ukiombe. "Endelea" inakufungulia cha kwanza. | draft | Flag: Drafter: 'Tap' = Gusa (touch-screen verb, no Tanzanian source); reviewer to confirm. |
| `tour.farmer.equipmentOpenTitle` | Open a machine | Fungua kifaa | draft |  |
| `tour.farmer.equipmentPriceBody` | The machine's rated power, its indicative price to buy and its indicative rent per day, where offered. A price is a guide, not a quotation, and nothing is ordered from this screen. | Nguvu iliyoandikwa ya mashine, bei yake elekezi ya kununua na kodi yake elekezi kwa siku, pale inapotolewa. Bei ni mwongozo, si nukuu ya bei, na hakuna kinachoagizwa kutoka skrini hii. | draft | Prices are INDICATIVE, never quotations. Must not read as a firm offer. |
| `tour.farmer.equipmentPriceTitle` | Power and price | Nguvu na bei | draft | Prices are INDICATIVE, never quotations. Must not read as a firm offer. |
| `tour.farmer.equipmentSubmitBody` | Submit request sends it to ops for review, and the tour never presses it. Until you send it, what you type is kept on this device as a draft, so a reload does not lose it. | Kitufe cha "Wasilisha ombi" kinalituma kwa timu ya uendeshaji lipitiwe, na ziara haikibonyezi kamwe. Hadi utakapolituma, unachoandika kinahifadhiwa kwenye kifaa hiki kama rasimu, kwa hiyo kupakia ukurasa upya hakukipotezi. | draft |  |
| `tour.farmer.equipmentSubmitTitle` | Sending the request | Kutuma ombi | draft |  |
| `tour.farmer.equipmentTryBody` | Change how many, hours per day or days per week and watch the estimate move. It is an estimate, not a measurement. A figure that is impossible, like 30 hours a day, hides it until you correct it. | Badilisha idadi, saa kwa siku au siku kwa wiki uone makadirio yakibadilika. Ni makadirio, si kipimo. Namba isiyowezekana, kama saa 30 kwa siku, huficha makadirio hadi uirekebishe. | draft |  |
| `tour.farmer.equipmentTryTitle` | Try your own figures | Jaribu namba zako | draft |  |
| `tour.farmer.homeLatestRequestBody` | The last equipment you asked for and where it stands — waiting, under review, approved or turned down. The Requests tab keeps the whole list. | Kifaa cha mwisho ulichoomba na kilipofikia — kinasubiri, kinapitiwa, kimeidhinishwa au kimekataliwa. Kichupo cha Maombi kina orodha nzima. | draft | Flag: Drafter: Statuses used as verbs (kinasubiri, kinapitiwa) matching requestStatus labels; 'Inapitiwa' is unverified in glossary. |
| `tour.farmer.homeLatestRequestTitle` | Your latest request | Ombi lako la karibuni | draft |  |
| `tour.farmer.homeOpportunitiesBody` | This counts the opportunities your expected harvest is part of. An opportunity is not a sale, a delivery or a payment — the Opportunities tab has the detail. | Hii inahesabu fursa zenye mavuno yako yanayotarajiwa. Fursa si mauzo, si uwasilishaji wa mazao wala si malipo — kichupo cha Fursa kina maelezo zaidi. | draft | Flag: Reworded after the cross-check; unreviewed. |
| `tour.farmer.homeOpportunitiesTitle` | Is your harvest in an opportunity? | Je, mavuno yako yamo katika fursa? | draft |  |
| `tour.farmer.myFarmBadgeBody` | This badge says who reported the record and whether an officer has checked it. Unverified is not the same as wrong. You can read your records here but not change them — ask your field officer. | Lebo hii inaonyesha nani aliripoti rekodi na kama afisa ameihakiki. Haijahakikiwa haimaanishi si sahihi. Unaweza kusoma rekodi zako hapa lakini huwezi kuzibadilisha — mwombe afisa wako wa uwandani. | draft |  |
| `tour.farmer.myFarmBadgeTitle` | Where every figure came from | Kila kiasi kilitoka wapi | draft |  |
| `tour.farmer.myFarmCropBody` | The harvest window, and the expected harvest in kilograms. Expected means planned, not weighed — the small badge under it says who reported it and whether it has been checked. | Kipindi cha kuvuna, na mavuno yanayotarajiwa kwa kilo (kg). Yanayotarajiwa yanamaanisha yaliyopangwa, si yaliyopimwa — lebo ndogo iliyo chini yake inaonyesha nani aliyeyaripoti na kama yamehakikiwa. | draft |  |
| `tour.farmer.myFarmCropTitle` | A crop and its harvest | Zao na mavuno yake | draft |  |
| `tour.farmer.myFarmPlotBody` | Each plot shows its area in hectares, with the crops grown there listed underneath. | Kila kipande cha shamba kinaonyesha eneo lake kwa hekta, na mazao yanayolimwa hapo yameorodheshwa chini yake. | draft |  |
| `tour.farmer.myFarmPlotTitle` | Plots and their crops | Vipande vya shamba na mazao yake | draft |  |
| `tour.farmer.nextFarmBody` | To collect the cash, the household shows the voucher at the Ruaha office. In the demo, sign in next as a field officer who neither registered nor verified this household: they scan the voucher and hand the cash over. | Ili kuchukua fedha taslimu, kaya inaonyesha vocha katika ofisi ya Ruaha. Katika demo, hatua inayofuata ni kuingia kama afisa wa uwandani ambaye hakusajili wala kuhakiki kaya hii: anaskani vocha na kukabidhi fedha taslimu. | draft | Flag: Drafter: 'demo' kept as a loanword; 'ingia' after 'hatua inayofuata ni kuingia' reads as instruction, not action. |
| `tour.farmer.nextFarmTitle` | The farmer side is done | Upande wa mkulima umekamilika | draft |  |
| `tour.farmer.opportunitiesBody` | An opportunity is a buyer looking for a crop, matched to what your village expects to harvest. It is not a sale, a delivery or a payment — nothing is owed either way. | Fursa ni mnunuzi anayetafuta zao, aliyelinganishwa na mavuno ambayo kijiji chako kinatarajia. Si uuzaji, si uwasilishaji wala malipo — hakuna anayedaiwa upande wowote. | draft | Flag: Drafter: Must say 'not a sale, a delivery or a payment'. Used 'uuzaji' for sale (avoiding mauzo/mkataba per glossary), 'uwasilishaji' for delivery, 'malipo' for payment. Native reviewer to confirm wording; 'malipo' is fine here (not an incentive string) but check any test scanning the whole file. |
| `tour.farmer.opportunitiesShareBody` | How much of your expected harvest is inside this opportunity, in kilograms. It has been put forward to a buyer — it has not been sold or delivered. | Kiasi cha mavuno yako yanayotarajiwa kilichomo katika fursa hii, kwa kilo (kg). Kimependekezwa kwa mnunuzi — hakijauzwa wala kufikishwa. | draft | Flag: Drafter: 'delivered' = 'kufikishwa' rather than 'kuwasilishwa', which also means 'submitted'; existing bundle uses 'uwasilishaji wa mazao' for delivery. Reviewer to pick one. Cross-check (low): 'kufikishwa' for 'delivered' differs from the bundle's 'uwasilishaji wa mazao' used in the not-a-sale sentences; same idea, different word. |
| `tour.farmer.opportunitiesShareTitle` | Your share | Sehemu yako | draft |  |
| `tour.farmer.opportunitiesTitle` | Opportunities near you | Fursa karibu nawe | draft |  |
| `tour.farmer.opportunitiesTotalBody` | The total across everyone whose harvest is in it, so it is never less than your share. The buyer's name is not shown here — your field officer holds those details. | Jumla ya kila mtu ambaye mavuno yake yamo ndani yake, kwa hiyo haiwezi kuwa chini ya sehemu yako. Jina la mnunuzi halionyeshwi hapa — afisa wako wa uwandani ana taarifa hizo. | draft |  |
| `tour.farmer.opportunitiesTotalTitle` | The whole opportunity | Fursa nzima | draft |  |
| `tour.farmer.recordsBody` | Each record carries a badge saying who recorded it and whether an officer has verified it. Unverified does not mean wrong; it means nobody has checked it yet. | Kila rekodi ina lebo inayoonyesha nani aliiandika na kama afisa ameihakiki. Haijahakikiwa haimaanishi si sahihi; inamaanisha bado hakuna aliyeikagua. | draft | Flag: Drafter: 'Badge' rendered as 'lebo' (my choice, not in glossary); glossary's ProvenanceBadge wording is 'chanzo cha taarifa'. 'Haijahakikiwa' [UNVERIFIED]. |
| `tour.farmer.recordsTitle` | Your farm, in detail | Shamba lako kwa undani | draft |  |
| `tour.farmer.requestsAssumptionsBody` | The figures this request was made with. A sent request cannot be edited: you can withdraw it until ops start reviewing, and after that your field officer can help with a correction. | Namba ambazo ombi hili liliwasilishwa nazo. Ombi lililotumwa haliwezi kubadilishwa: unaweza kuliondoa hadi timu ya uendeshaji ianze kulipitia, na baada ya hapo afisa wako wa uwandani anaweza kusaidia kurekebisha. | draft | Flag: Reworded after the cross-check; unreviewed. |
| `tour.farmer.requestsAssumptionsTitle` | What you asked for | Ulichoomba | draft |  |
| `tour.farmer.requestsBody` | Every request you send appears here with its current status, so you can see whether it is waiting, under review, approved or turned down — and why. | Kila ombi unalotuma linaonekana hapa likiwa na hali yake ya sasa, ili uone kama linasubiri, linapitiwa, limeidhinishwa au limekataliwa — na kwa nini. | draft |  |
| `tour.farmer.requestsDecisionBody` | When ops approve a request or turn it down, their note and the date appear here, so you can see the reason. | Timu ya uendeshaji inapoidhinisha ombi au kulikataa, maelezo yake na tarehe vinaonekana hapa, ili uone sababu. | draft |  |
| `tour.farmer.requestsDecisionTitle` | What ops decided | Uamuzi wa timu ya uendeshaji | draft |  |
| `tour.farmer.requestsEstimateBody` | The estimate saved with your request: peak power, kWh a day and kWh a week. It was worked out from the figures above, and it is an estimate, not a measurement. | Makadirio yaliyohifadhiwa pamoja na ombi lako: kilele kilichokadiriwa, kWh kwa siku na kWh kwa wiki. Yalikokotolewa kutokana na namba zilizo hapo juu, na ni makadirio, si kipimo. | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. |
| `tour.farmer.requestsEstimateTitle` | The estimate on file | Makadirio yaliyohifadhiwa | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. |
| `tour.farmer.requestsTitle` | What you have asked for | Ulichoomba | draft |  |
| `tour.farmer.summaryBody` | Your farm, your plots and what you expect to harvest. If something here is wrong, tell your field officer — they are the one who can correct it. | Shamba lako, vipande vya shamba lako na mavuno yanayotarajiwa. Kama kuna kitu kisicho sahihi hapa, mwambie afisa wako wa uwandani — ndiye anayeweza kukirekebisha. | draft |  |
| `tour.farmer.summaryTitle` | What is on file for you | Kilichoandikwa kukuhusu | draft |  |
| `tour.farmer.surveysBody` | Answer a survey and your household receives a fixed cash incentive, once per survey. You get a voucher code; show it at the Ruaha office to collect the cash. The number on this tab is how many surveys are waiting for you. | Jibu dodoso na kaya yako itapokea motisha ya kiasi maalum cha fedha taslimu, mara moja kwa kila dodoso. Utapata namba ya vocha; ionyeshe kwenye ofisi ya Ruaha ili uchukue fedha hizo. Namba iliyo kwenye kichupo hiki ni idadi ya madodoso yanayokusubiri. | draft | Flag: Drafter: Incentive string: motisha, vocha, kiasi maalum cha fedha taslimu used; none of the banned words present. 'Namba ya vocha' and 'kichupo' (tab) are composed [UNVERIFIED]. |
| `tour.farmer.surveysCardBody` | New means you can answer it now. Answered keeps your voucher. Not available gives the reason — for example, that the survey has closed. | Jipya linamaanisha unaweza kulijibu sasa. Limejibiwa hutunza vocha yako. Halipatikani hutoa sababu — kwa mfano, dodoso limefungwa. | draft |  |
| `tour.farmer.surveysCardTitle` | One card for each survey | Kadi moja kwa kila dodoso | draft |  |
| `tour.farmer.surveysIncentiveBody` | A fixed cash amount for your household, once per survey. You collect it at the Ruaha office by showing the voucher you get when you answer. | Kiasi maalum cha fedha taslimu kwa kaya yako, mara moja kwa kila dodoso. Unakichukua katika ofisi ya Ruaha kwa kuonyesha vocha unayopata unapojibu. | draft |  |
| `tour.farmer.surveysIncentiveTitle` | The incentive | Motisha | draft |  |
| `tour.farmer.surveysOpenBody` | Answer survey opens the questions. Next opens the first new survey for you — nothing is sent until you submit your answers. | "Jibu dodoso" inafungua maswali. "Endelea" inakufungulia dodoso la kwanza jipya — hakuna kinachotumwa hadi uwasilishe majibu yako. | draft |  |
| `tour.farmer.surveysOpenTitle` | Open a survey | Fungua dodoso | draft |  |
| `tour.farmer.surveysQuestionsBody` | Each question says whether it is required. Answers can be a choice, yes or no, a number or a few words. Your household answers each survey once, so check before you submit. | Kila swali linaonyesha kama ni lazima. Majibu yanaweza kuwa chaguo, ndiyo au hapana, namba au maneno machache. Kaya yako hujibu kila dodoso mara moja, kwa hiyo kagua kabla ya kuwasilisha. | draft |  |
| `tour.farmer.surveysQuestionsTitle` | Answering a survey | Kujibu dodoso | draft |  |
| `tour.farmer.surveysSubmitBody` | Submit answers is final for this survey, and your voucher is issued as soon as it is sent. The tour never presses it. What you type is kept on this device until you send it. | "Wasilisha majibu" ni ya mwisho kwa dodoso hili, na vocha yako inatolewa mara tu majibu yanapotumwa. Ziara haibonyezi kitufe hicho kamwe. Unachoandika kinahifadhiwa kwenye kifaa hiki hadi utakapotuma. | draft |  |
| `tour.farmer.surveysSubmitTitle` | Sending your answers | Kutuma majibu yako | draft |  |
| `tour.farmer.surveysTitle` | Surveys, and the incentive for answering | Madodoso, na motisha ya kujibu | draft |  |
| `tour.farmer.voucherAmountBody` | The fixed cash amount your household collects at the Ruaha office for this survey. It is set by the survey, not by how you answered. | Kiasi maalum cha fedha taslimu ambacho kaya yako inachukua katika ofisi ya Ruaha kwa dodoso hili. Kinawekwa na dodoso, si na jinsi ulivyojibu. | draft |  |
| `tour.farmer.voucherAmountTitle` | The amount | Kiasi | draft |  |
| `tour.farmer.voucherCodeBody` | Staff at the Ruaha office scan the QR code or type this code. Only your household can see it. | Wafanyakazi wa ofisi ya Ruaha wanaskani msimbo wa QR au wanaandika namba hii. Kaya yako pekee ndiyo inayoweza kuiona. | draft |  |
| `tour.farmer.voucherCodeTitle` | The code | Msimbo | draft |  |
| `tour.farmer.voucherSlipBody` | A claim slip for a fixed cash amount — not money held in the app. While it waits to be collected it carries a QR code for staff at the Ruaha office to scan, and it works once. | Hati ya kudai kiasi maalum cha fedha taslimu — si fedha zilizohifadhiwa kwenye programu. Inaposubiri kuchukuliwa ina msimbo wa QR ambao wafanyakazi wa ofisi ya Ruaha wanaskani, na inatumika mara moja. | draft | Flag: Drafter: 'claim slip' = 'hati ya kudai' is a composed term; no Tanzanian source. 'Not money held in the app' avoids pochi/salio. Cross-check (low): 'hati ya kudai' (kudai also means to demand a debt owed) is an unverified composed term; the meaning and rule (fixed cash, not money held in the app) are preserved. |
| `tour.farmer.voucherSlipTitle` | Your voucher | Vocha yako | draft |  |
| `tour.farmer.voucherTrailBody` | Each step on this voucher, with names: when you answered, when it was issued and, once collected, who handed the incentive over. The record cannot be changed. | Kila hatua kwenye vocha hii, pamoja na majina: ulipojibu, ilipotolewa, na — ikishachukuliwa — nani aliyekabidhi motisha. Rekodi hii haiwezi kubadilishwa. | draft | Flag: Reworded after the cross-check; unreviewed. |
| `tour.farmer.voucherTrailTitle` | Who did what | Nani alifanya nini | draft |  |
| `tour.farmer.voucherValidityBody` | Until it is collected, this line gives the date the voucher stays valid. Once collected, it gives the date and the name of the person who handed the cash over. | Hadi itakapochukuliwa, mstari huu unaonyesha tarehe ambayo vocha inaendelea kutumika. Ikishachukuliwa, unaonyesha tarehe na jina la mtu aliyekabidhi fedha. | draft |  |
| `tour.farmer.voucherValidityTitle` | Valid until, then handed over | Inatumika hadi, kisha imekabidhiwa | draft | Flag: Cross-check (low): 'Inatumika hadi, kisha imekabidhiwa' is an unfinished clause ('until' with no date) and mixes a voucher subject with a cash hand-over; mirrors the English but sounds clumsy. |
| `tour.farmer.voucherViewBody` | Every survey you have answered keeps a View voucher button. Next opens the first one. | Kila dodoso ulilolijibu lina kitufe cha "Angalia vocha". "Endelea" inafungua cha kwanza. | draft |  |
| `tour.farmer.voucherViewTitle` | Open a voucher | Fungua vocha | draft |  |
| `tour.farmer.welcomeBody` | This is your own record. Everything here was written by a field officer who visited your farm, and every figure shows where it came from. The tour button in the header (the person icon on a phone) shows you any part of the app whenever you want. | Hii ni rekodi yako mwenyewe. Kila kitu hapa kiliandikwa na afisa wa uwandani aliyetembelea shamba lako, na kila kiasi kinaonyesha chanzo chake. Kitufe cha Ziara kilicho juu ya ukurasa (ikoni ya mtu kwenye simu) kinakuonyesha sehemu yoyote ya programu wakati wowote unapotaka. | draft | Flag: Drafter: 'Tour button' rendered as 'kitufe cha Ziara'; the header label's own Swahili (tour.restart 'Rudia ziara') differs slightly. Confirm what the button shows. Cross-check (low): Says 'Kitufe cha Ziara' but the header button reads 'Rudia ziara' (tour.restart), so the quoted name differs from the screen (the English is also loose here). |
| `tour.farmer.welcomeTabsBody` | The tabs along the bottom open My farm, Equipment, Requests, Opportunities and Surveys — Surveys shows a number when one is waiting. The Tour button in the header replays any part of this tour. | Vichupo vilivyo chini vinafungua Shamba langu, Vifaa, Maombi, Fursa na Madodoso — Madodoso huonyesha namba wakati dodoso linakusubiri. Kitufe cha Ziara kilicho juu kinarudia sehemu yoyote ya ziara hii. | draft | Flag: Drafter: 'tabs' = vichupo (same as existing tour strings); 'a number when one is waiting' compressed to 'huonyesha namba dodoso linapokusubiri'. Reworded after the cross-check; unreviewed. |
| `tour.farmer.welcomeTabsTitle` | Finding your way around | Kujua pa kwenda | draft |  |
| `tour.farmer.welcomeTitle` | Welcome to Ruaha 360 | Karibu Ruaha 360 | draft |  |
| `tour.gateHint` | Do this first, then Next unlocks. | Fanya hivi kwanza, kisha "Endelea" itafunguka. | draft | Flag: Drafted after the main cross-check, for tour strings added late (chapter menu). Not back-translated. Unreviewed. |
| `tour.menu.all` | Play the whole tour | Anza ziara yote | draft | Flag: Drafted after the main cross-check, for tour strings added late (chapter menu). Not back-translated. Unreviewed. |
| `tour.menu.close` | Close | Funga | draft | Flag: Drafted after the main cross-check, for tour strings added late (chapter menu). Not back-translated. Unreviewed. |
| `tour.menu.title` | Choose a part of the tour | Chagua sehemu ya ziara | draft | Flag: Drafted after the main cross-check, for tour strings added late (chapter menu). Not back-translated. Unreviewed. |
| `tour.next` | Next | Endelea | draft | Flag: Changed to the imperative "Endelea" (also "Continue"). Reviewer may prefer another word for "Next". Drafter: Glossary gives 'Inayofuata' [UNVERIFIED]; it is not a bare imperative. 'Endelea' (continue) is the attested alternative. Cross-check (glossary): 'Inayofuata' is not a bare imperative (back-translation 'The next one'), against the editorial decision that buttons are bare imperatives; drafter also flagged it. |
| `tour.officer.chapter.home` | Your home screen | Skrini yako ya nyumbani | draft | Flag: Drafted after the main cross-check, for tour strings added late (chapter menu). Not back-translated. Unreviewed. |
| `tour.officer.chapter.next` | What happens next | Kinachofuata | draft | Flag: Drafted after the main cross-check, for tour strings added late (chapter menu). Not back-translated. Unreviewed. |
| `tour.officer.chapter.people` | People and their records | Watu na rekodi zao | draft | Flag: Drafted after the main cross-check, for tour strings added late (chapter menu). Not back-translated. Unreviewed. |
| `tour.officer.chapter.redeem` | Redeeming a voucher | Kukabidhi vocha | draft | Flag: Drafted after the main cross-check, for tour strings added late (chapter menu). Not back-translated. Unreviewed. |
| `tour.officer.chapter.register` | Registering a farmer | Kusajili mkulima | draft | Flag: Drafted after the main cross-check, for tour strings added late (chapter menu). Not back-translated. Unreviewed. |
| `tour.officer.chapter.verify` | Verifying records | Kuhakiki rekodi | draft | Flag: Drafted after the main cross-check, for tour strings added late (chapter menu). Not back-translated. Unreviewed. |
| `tour.officer.chapter.welcome` | Welcome | Karibu | draft | Flag: Drafted after the main cross-check, for tour strings added late (chapter menu). Not back-translated. Unreviewed. |
| `tour.officer.homeRegisterBody` | Your main job. This opens one page that records a whole farmer: person, household, farm, plot, crop cycle and expected harvest. Everything you record starts unverified. | Kazi yako kuu. Hii inafungua ukurasa mmoja unaorekodi mkulima mzima: mtu, kaya, shamba, kipande cha shamba, msimu wa zao na mavuno yanayotarajiwa. Kila unachorekodi huanza kikiwa hakijahakikiwa. | draft |  |
| `tour.officer.homeRegisterTitle` | Register a farmer | Sajili mkulima | draft |  |
| `tour.officer.homeUnverifiedBody` | This counts records in your villages that still need verifying, including everything you just registered. The button opens the queue. A household you registered needs a second staff member to verify it, so it stays counted until they do. | Hii inahesabu rekodi katika vijiji vyako ambazo bado zinahitaji kuhakikiwa, pamoja na kila ulichosajili sasa hivi. Kitufe kinafungua orodha. Kaya uliyoisajili inahitaji mfanyakazi mwingine kuihakiki, kwa hiyo inaendelea kuhesabiwa hadi afanye hivyo. | draft |  |
| `tour.officer.homeVillageBody` | The village you are assigned to, with how many people, farms and equipment requests are recorded there. | Kijiji ulichopangiwa, pamoja na idadi ya watu, mashamba na maombi ya vifaa yaliyorekodiwa hapo. | draft |  |
| `tour.officer.homeVillageTitle` | Your village | Kijiji chako | draft |  |
| `tour.officer.nextBody` | Ask a second staff member to verify your household, and an admin to publish a survey after it. Sign in as the farmer with the phone and temporary password from the login card and answer it. Then redeem the voucher as an officer who neither registered nor verified that household. | Mwombe mfanyakazi mwingine ahakiki kaya yako, kisha msimamizi achapishe dodoso. Ingia kama mkulima kwa simu na neno la siri la muda kutoka kwenye kadi ya kuingia kisha ulijibu. Halafu kabidhi vocha ukiwa afisa ambaye hakusajili wala kuhakiki kaya hiyo. | draft | Flag: Drafter: 'achapishe' (publish a survey): bundle shows survey status 'Linaendelea' but no verb for publish; 'Chapisha' is the plain choice, verify it matches the admin Publish button. |
| `tour.officer.nextTitle` | See it through live | Iona hadi mwisho moja kwa moja | draft |  |
| `tour.officer.outstandingTitle` | What still needs verifying | Kinachosubiri kuhakikiwa | draft |  |
| `tour.officer.peopleDetailTitle` | Everything about one person | Kila kitu kuhusu mtu mmoja | draft |  |
| `tour.officer.peopleEditBody` | Edit fixes a wrong detail on a record. The correction is recorded under your name and sends that record back to unverified, so it must be verified again. | Hariri hurekebisha taarifa isiyo sahihi kwenye rekodi. Marekebisho yanarekodiwa kwa jina lako na kurudisha rekodi hiyo kuwa haijahakikiwa, kwa hiyo lazima ihakikiwe tena. | draft |  |
| `tour.officer.peopleEditTitle` | Correcting a record | Kurekebisha rekodi | draft |  |
| `tour.officer.peopleFindBody` | Type part of a first or family name, or filter by verification status, and the list follows. Clear the box before you go on. | Andika sehemu ya jina la kwanza au la ukoo, au chuja kwa hali ya uhakiki, na orodha itafuata. Futa maandishi kwenye kisanduku kabla ya kuendelea. | draft |  |
| `tour.officer.peopleFindTitle` | Search and filter | Tafuta na chuja | draft |  |
| `tour.officer.peopleListTitle` | One row per person | Mstari mmoja kwa kila mtu | draft | Flag: Drafter: 'Mstari' for table row (glossary has no term); kept 'mstari' in related body strings. |
| `tour.officer.peopleLoginBody` | Create the farmer's app login here, or reset it if they forget their password, which signs them out everywhere. The temporary password shows once. This card records who issued it and when, and whether the farmer still has to choose their own. | Unda akaunti ya programu ya mkulima hapa, au rudisha neno lake la siri ikiwa amelisahau, jambo linalomtoa kwenye kila kifaa alichoingia. Neno la siri la muda huonyeshwa mara moja. Kadi hii inarekodi nani aliyeitoa na lini, na kama mkulima bado anapaswa kuchagua neno lake mwenyewe. | draft | Flag: Drafter: 'kadi hii' = login card, shortened; 'jambo linalomtoa kwenye kila kifaa alichoingia' for 'signs them out everywhere' is composed. Reworded after the cross-check; unreviewed. |
| `tour.officer.peopleLoginTitle` | The farmer's app login | Akaunti ya programu ya mkulima | draft | Flag: Reworded after the cross-check; unreviewed. |
| `tour.officer.peopleOutstandingBody` | How many of this person's records still need verifying. Each record has its own Verify button: one decision at a time, and it cannot be undone. | Idadi ya rekodi za mtu huyu ambazo bado zinahitaji kuhakikiwa. Kila rekodi ina kitufe chake cha Hakiki: uamuzi mmoja kwa wakati, na hauwezi kutenguliwa. | draft | Flag: Reworded after the cross-check; unreviewed. |
| `tour.officer.peopleOutstandingTitle` | What is left to verify | Kilichobaki kuhakikiwa | draft |  |
| `tour.officer.peoplePersonBody` | Households, farms, plots, crop cycles and harvest figures. Each has a badge showing where its data came from, when it was captured and whether it is verified. | Kaya, mashamba, vipande vya shamba, misimu ya mazao na kiasi cha mavuno. Kila kimoja kina lebo inayoonyesha taarifa zilitoka wapi, zilikusanywa lini na kama zimehakikiwa. | draft | Flag: Reworded after the cross-check; unreviewed. |
| `tour.officer.peopleRowsBody` | Each row shows the phone, the village and a badge saying where the data came from and whether it is verified. Next opens the first person for you. | Kila mstari unaonyesha simu, kijiji na lebo inayosema taarifa zilitoka wapi na kama zimehakikiwa. Endelea inakufungulia mtu wa kwanza. | draft | Flag: Reworded after the cross-check; unreviewed. |
| `tour.officer.redeemBody` | Scan the farmer's voucher, check their ID and hand over the cash. You cannot redeem a voucher for a household you registered or verified, and some vouchers are held for ops to redeem in person. Every scan is recorded with your name. | Skani vocha ya mkulima, angalia kitambulisho chake kisha kabidhi fedha taslimu. Huwezi kukabidhi vocha ya kaya uliyoisajili au kuihakiki, na baadhi ya vocha zimeshikiliwa ili uendeshaji uzikabidhi ana kwa ana. Kila skani inarekodiwa pamoja na jina lako. | draft | Flag: Drafter: Officer-side redeem rendered as 'kabidhi' (hand over) per glossary; 'ops' rendered 'uendeshaji' [UNVERIFIED role label]. 'Skani' per glossary. |
| `tour.officer.redeemCodeBody` | If the camera isn't available or the code won't scan, type the code shown under the QR code. It works exactly the same way. | Kamera isipopatikana au msimbo usiposomeka, andika namba iliyo chini ya msimbo wa QR. Inafanya kazi kwa njia ile ile kabisa. | draft |  |
| `tour.officer.redeemCodeTitle` | Or type the code | Au andika namba | draft |  |
| `tour.officer.redeemLookupBody` | Look up shows the household and the fixed cash amount. You then record which ID you saw and confirm the name matches, never the number. If you may not redeem it, it says why. Every look-up is logged with your name. | Tafuta inaonyesha kaya na kiasi maalum cha fedha taslimu. Kisha unarekodi aina ya kitambulisho ulichoona na kuthibitisha kuwa jina linalingana, kamwe si namba. Kama huruhusiwi kukabidhi, inaeleza sababu. Kila utafutaji unarekodiwa kwa jina lako. | draft |  |
| `tour.officer.redeemLookupTitle` | Look up before you hand over | Tafuta kabla ya kukabidhi | draft |  |
| `tour.officer.redeemScanBody` | The farmer shows their voucher's QR code on their phone. Tap here, point the camera at it and the code is read for you. The QR code is a single-use claim slip. | Mkulima anaonyesha msimbo wa QR wa vocha yake kwenye simu yake. Gusa hapa, elekeza kamera kwenye msimbo na utasomwa kwa ajili yako. Msimbo wa QR ni hati ya kudai inayotumika mara moja tu. | draft | Flag: Drafter: 'hati ya kudai' for 'claim slip' is my composition; native reviewer to confirm. |
| `tour.officer.redeemScanTitle` | Scan the QR code | Skani msimbo wa QR | draft |  |
| `tour.officer.redeemTitle` | Hand over a survey incentive | Kabidhi motisha ya dodoso | draft |  |
| `tour.officer.registerBody` | Six groups — person, household, farm, plot, crop cycle, expected harvest — created together in a single step. These chips show which groups you have filled in. | Makundi sita — mtu, kaya, shamba, kipande cha shamba, msimu wa zao, mavuno yanayotarajiwa — yanaundwa pamoja kwa kuwasilisha mara moja. Alama hizi zinaonyesha makundi uliyojaza. | draft | Flag: Drafter: 'Chips' rendered as 'alama' (marks); six group names composed from glossary terms (msimu wa zao, kipande cha shamba, mavuno yanayotarajiwa are all [UNVERIFIED]). |
| `tour.officer.registerConfidenceBody` | Choose low, medium or high: your own judgement of how firm the details are. It appears on each record's badge afterwards. | Chagua chini, wastani au juu: ni maoni yako mwenyewe kuhusu jinsi taarifa zilivyo na uhakika. Baadaye huonekana kwenye lebo ya kila rekodi. | draft | Flag: Drafter: 'alama' used for the record badge (no bundle precedent for 'badge'); 'maoni yako mwenyewe' for 'your own judgement' is plain but unchecked. Reworded after the cross-check; unreviewed. |
| `tour.officer.registerConfidenceTitle` | How sure are you? | Una uhakika kiasi gani? | draft |  |
| `tour.officer.registerDraftBody` | Fill in a first and family name. Nothing is registered: what you type is kept as a draft on this phone only. Reload the page and it is still there. | Jaza jina la kwanza na jina la ukoo. Hakuna kinachosajiliwa: unachoandika kinahifadhiwa kama rasimu kwenye simu hii pekee. Pakia ukurasa upya na bado kipo. | draft | Flag: Drafter: 'Pakia ukurasa upya' for 'reload the page'; 'pakia' is unverified in glossary. |
| `tour.officer.registerDraftTitle` | Type a made-up name | Andika jina la kubuni | draft |  |
| `tour.officer.registerFarmBody` | This phone's GPS fills in the latitude and longitude once you allow location. If it can't, try again or type the coordinates yourself. | GPS ya simu hii hujaza latitudo na longitudo baada ya kuruhusu simu ipate mahali. Ikishindwa, jaribu tena au andika viwianishi mwenyewe. | draft | Flag: Reworded after the cross-check; unreviewed. |
| `tour.officer.registerFarmTitle` | The farm's position is required | Mahali pa shamba ni lazima | draft |  |
| `tour.officer.registerPhoneBody` | Type it with the country code. After you register, this number is the farmer's app login. They sign in with it, so check it with them. | Iandike pamoja na msimbo wa nchi. Baada ya kusajili, namba hii ni akaunti ya programu ya mkulima. Ataingia kwa namba hii, kwa hiyo iangalie naye. | draft | Flag: Reworded after the cross-check; unreviewed. |
| `tour.officer.registerPhoneTitle` | The phone number is required | Namba ya simu ni lazima | draft |  |
| `tour.officer.registerSubmitBody` | One press creates all six records together, and the farmer's app login. A card then shows their phone and a temporary password, once only, so write it down. The tour never presses this for you. | Kubonyeza mara moja huunda rekodi zote sita pamoja, na akaunti ya programu ya mkulima. Kisha kadi inaonyesha simu yake na neno la siri la muda, mara moja tu, kwa hiyo liandike. Ziara haibonyezi kitufe hiki kwa niaba yako. | draft | Flag: Reworded after the cross-check; unreviewed. |
| `tour.officer.registerSubmitTitle` | Register creates it all at once | Sajili huunda yote kwa pamoja | draft |  |
| `tour.officer.registerTitle` | One page, one submit | Ukurasa mmoja, kuwasilisha mara moja | draft |  |
| `tour.officer.verifyBody` | Everything awaiting verification, in one list. Verifying says you have seen the record yourself. There is no way to un-verify, so leave anything you are unsure of. | Kila kinachosubiri kuhakikiwa, kwenye orodha moja. Kuhakiki kunamaanisha umeiona rekodi mwenyewe. Hakuna njia ya kubatilisha, kwa hiyo acha chochote ambacho huna uhakika nacho. | draft |  |
| `tour.officer.verifyCountBody` | Records in your villages that nobody has verified yet: people, households, farms, plots, crop cycles and harvest figures. The number falls as they are verified. | Rekodi katika vijiji vyako ambazo hakuna aliyezihakiki bado: watu, kaya, mashamba, vipande vya shamba, misimu ya mazao na kiasi cha mavuno. Namba hupungua kadri zinavyohakikiwa. | draft |  |
| `tour.officer.verifyCountTitle` | How many are waiting | Ngapi zinasubiri | draft |  |
| `tour.officer.verifyRowCheckBody` | Check the record first: most rows open it when tapped. Verify attaches your name, asks you to confirm and cannot be undone. A household you registered shows a note instead: another staff member must verify it. | Kagua rekodi kwanza: mistari mingi hufungua rekodi ikiguswa. Hakiki inaambatisha jina lako, inakuomba uthibitishe na haiwezi kutenguliwa. Kaya uliyoisajili huonyesha ujumbe badala yake: mfanyakazi mwingine lazima aihakiki. | draft | Flag: Reworded after the cross-check; unreviewed. |
| `tour.officer.verifyRowTitle` | Verify means you saw it yourself | Hakiki inamaanisha uliona mwenyewe | draft |  |
| `tour.officer.verifyTitle` | The verify queue | Foleni ya kuhakiki | draft | Flag: Drafter: 'Foleni' (queue) is my choice; alternative 'Orodha ya kuhakiki'. |
| `tour.officer.welcomeBody` | This is the field officer's surface. You register farmers, verify what you recorded, and hand over survey incentives. The tour button in the header (the person icon on a phone) shows you any part of it again whenever you want. | Hii ni sehemu ya afisa wa uwandani. Unasajili wakulima, unahakiki ulichorekodi, na unakabidhi motisha za madodoso. Kitufe cha ziara juu ya skrini (alama ya mtu kwenye simu) kinakuonyesha sehemu yoyote tena wakati wowote unapotaka. | draft | Flag: Drafter: 'kitufe cha ziara' / 'alama ya mtu' (person icon): 'alama' used for icon and for badge; 'ziara' for tour is unverified in glossary. |
| `tour.officer.welcomeTabsBody` | Register adds a farmer. People finds and opens their records. Verify clears what nobody has checked yet. Redeem hands over a survey incentive. The Tour button in the header replays any part. | Sajili huongeza mkulima. Watu hukuwezesha kutafuta na kufungua rekodi zao. Hakiki huondoa yale ambayo hakuna aliyeyakagua bado. Kabidhi hutumika kukabidhi motisha ya dodoso. Kitufe cha Ziara juu ya skrini hurudia sehemu yoyote. | draft | Flag: Reworded after the cross-check; unreviewed. |
| `tour.officer.welcomeTabsTitle` | Four tabs, one job each | Vichupo vinne, kazi moja kila kimoja | draft |  |
| `tour.officer.welcomeTitle` | Welcome to Ruaha 360 | Karibu Ruaha 360 | draft |  |
| `tour.progress` | Step {{step}} of {{total}} | Hatua {{step}} kati ya {{total}} | draft | Keep {{step}} {{total}} |
| `tour.restart` | Take the tour again | Rudia ziara | draft | Flag: Changed after the cross-check; unreviewed. |
| `tour.skip` | Skip the tour | Ruka ziara | draft |  |
| `tour.tryIt` | Try it | Jaribu | draft | Flag: Drafted after the main cross-check, for tour strings added late (chapter menu). Not back-translated. Unreviewed. |
| `verification.disputed` | Disputed | Imepingwa | draft | Provenance wording: where a record came from, and who verified it. Flag: Drafter: 'Imepingwa' (contested) is not in glossary; plain wording chosen. Alternative 'Ina mgogoro'. |
| `verification.pending` | Pending | Inasubiri | draft | Provenance wording: where a record came from, and who verified it. Flag: Drafter: 'Inasubiri' unverified per glossary. |
| `verification.unverified` | Unverified | Haijahakikiwa | draft | Provenance wording: where a record came from, and who verified it. Flag: Drafter: 'Haijahakikiwa' unverified per glossary. |
| `verification.verified` | Verified | Imehakikiwa | draft | Provenance wording: where a record came from, and who verified it. |
| `verifyDialog.cancel` | Cancel | Ghairi | draft | Flag: Drafter: 'Ghairi' UNVERIFIED (glossary). Same for officerEdit.cancel. |
| `verifyDialog.confirm` | Verify record | Hakiki rekodi | draft |  |
| `verifyDialog.detail` | You are confirming {{record}}. Your name will be attached, and this action cannot be undone. | Unahakiki {{record}}. Jina lako litaambatishwa, na kitendo hiki hakiwezi kutenguliwa. | draft | Keep {{record}} Flag: Drafter: 'Jina lako litaambatishwa' and 'hakiwezi kutenguliwa' composed; check natural phrasing for 'cannot be undone'. Cross-check (glossary): English says 'confirming' (Thibitisha in the glossary) but Swahili uses 'Unahakiki' (Hakiki = verify). This keeps the dialog consistent with its Verify title and button, but breaks the locked Hakiki/Thibitisha split, so the product owner should decide deliberately. |
| `verifyDialog.title` | Verify this record? | Hakiki rekodi hii? | draft |  |
| `verifyQueue.householdNeedsSecondStaff` | You registered this household, so another staff member must verify it. | Ulisajili kaya hii, kwa hiyo mfanyakazi mwingine lazima aihakiki. | draft | Flag: Drafter: 'Mfanyakazi' used for 'staff member'; alternative 'mwenzako wa kazi'. Avoided 'wewe' via verb prefix. |
| `verifyQueue.intro` | Records captured in your villages that nobody has checked yet. Verifying attaches your name to the record. | Rekodi zilizokusanywa katika vijiji vyako ambazo bado hakuna aliyezikagua. Kuhakiki kunaambatisha jina lako kwenye rekodi. | draft | Flag: Drafter: 'Kuhakiki kunaambatisha jina lako' composed; 'ambatisha' (attach) not in glossary. |
| `verifyQueue.noneDetail` | Every record in your villages has been checked. | Rekodi zote katika vijiji vyako zimekaguliwa. | draft |  |
| `verifyQueue.noneTitle` | Nothing waiting to be verified | Hakuna kinachosubiri kuhakikiwa | draft |  |
| `verifyQueue.outstanding_one` | 1 record waiting | Rekodi 1 inasubiri | draft |  |
| `verifyQueue.outstanding_other` | {{count}} records waiting | Rekodi {{count}} zinasubiri | draft | Keep {{count}} |
| `verifyQueue.table.crop_cycle` | Crop cycle | Msimu wa zao | draft | Flag: Drafter: 'Msimu wa zao' unverified per glossary. |
| `verifyQueue.table.farm` | Farm | Shamba | draft |  |
| `verifyQueue.table.harvest_report` | Harvest figure | Kiasi cha mavuno | draft |  |
| `verifyQueue.table.household` | Household | Kaya | draft |  |
| `verifyQueue.table.person` | Person | Mtu | draft |  |
| `verifyQueue.table.plot` | Plot | Kipande | draft | Flag: Drafter: 'Kipande cha shamba' unverified per glossary. |
| `verifyQueue.title` | Verify | Hakiki | draft |  |
| `voucher.amount` | Incentive | Motisha | draft |  |
| `voucher.code` | Voucher code | Namba ya vocha | draft |  |
| `voucher.expired` | This voucher expired on {{date}}. | Vocha hii iliisha muda wake tarehe {{date}}. | draft | Keep {{date}} Flag: Drafter: 'expired' phrasing 'iliisha muda wake' UNVERIFIED in glossary. |
| `voucher.expires` | Valid until {{date}} | Inatumika hadi {{date}} | draft | Keep {{date}} Flag: Drafter: 'Valid until' -> 'Inatumika hadi' (usable until); composed. |
| `voucher.qrLabel` | QR code for voucher {{code}} | Msimbo wa QR wa vocha {{code}} | draft | Keep {{code}} |
| `voucher.redeemedBy` | Collected {{date}} · handed over by {{name}} | Imechukuliwa {{date}} · imekabidhiwa na {{name}} | draft | Keep {{date}} {{name}} Flag: Drafter: 'Collected' = 'Imechukuliwa' (glossary UNVERIFIED) paired with 'imekabidhiwa' (handed over) for the officer; 'Collected/handed over' distinction mirrors English. |
| `voucher.showAtOffice` | Show this code at the Ruaha office. A staff member scans it and hands over the incentive. It can be used once. | Onyesha msimbo huu katika ofisi ya Ruaha. Mfanyakazi ataskani na kukukabidhi motisha. Vocha hii inaweza kutumika mara moja. | draft | Flag: Drafter: 'staff member' -> 'Mfanyakazi' (no glossary term); 'ataskani' verb form from loan 'skani' unattested. 'Vocha hii inaweza kutumika mara moja' used for 'It can be used once'. |
| `voucher.title` | Your incentive voucher | Vocha yako ya motisha | draft |  |
| `voucher.voided` | Cancelled: {{reason}} | Imeghairiwa: {{reason}} | draft | Keep {{reason}} |
| `voucherStatus.expired` | Expired | Imeisha muda wake | draft |  |
| `voucherStatus.issued` | Not yet collected | Haijachukuliwa bado | draft |  |
| `voucherStatus.redeemed` | Collected | Imechukuliwa | draft |  |
| `voucherStatus.void` | Cancelled | Imeghairiwa | draft |  |

## Optional — ops and tower surfaces

| Key | English | Swahili | Status | Notes |
| --- | --- | --- | --- | --- |
| `buyers.channel.afm` | AFM | AFM | draft |  |
| `buyers.channel.direct` | Direct | Moja kwa moja | draft |  |
| `buyers.channel.other` | Other | Nyingine | draft |  |
| `buyers.channelNote` | Channel is a label describing how the buyer was reached. 'AFM' records that origin only — there is no integration and no partnership implied. | Njia ni lebo inayoeleza jinsi mnunuzi alivyofikiwa. 'AFM' inarekodi asili hiyo tu — hakuna muunganisho wa kimfumo na hakuna ushirikiano unaodokezwa. | draft | Flag: Drafter: 'muunganisho wa kimfumo' for 'integration' is composed; the not-implied-partnership caveat kept in full. |
| `buyers.colActive` | Active | Hai | draft | Flag: Drafter: 'Hai' (alive/active) for Active flag; alternative 'Anatumika'. |
| `buyers.colChannel` | Channel | Njia | draft | Flag: Drafter: 'Njia' (way/route) for Channel; alternative 'Chaneli'. Composed sense: how the buyer was reached. |
| `buyers.colContact` | Contact note | Maelezo ya mawasiliano | draft | Flag: Cross-check (low): English is 'Contact note' (free-text note); 'Maelezo ya mawasiliano' back-translates as 'Contact details', implying structured contact data rather than a note. |
| `buyers.colName` | Buyer | Mnunuzi | draft |  |
| `buyers.create` | Add buyer | Ongeza mnunuzi | draft |  |
| `buyers.createTitle` | Add a buyer | Ongeza mnunuzi | draft |  |
| `buyers.creating` | Adding… | Inaongeza… | draft |  |
| `buyers.nameRequired` | A buyer needs a name. | Mnunuzi anahitaji jina. | draft |  |
| `buyers.noneDetail` | Add the first buyer to record demand against them. | Ongeza mnunuzi wa kwanza ili kurekodi mahitaji yake. | draft |  |
| `buyers.noneTitle` | No buyers yet | Bado hakuna wanunuzi | draft |  |
| `buyers.title` | Buyers | Wanunuzi | draft |  |
| `catalogue.add.equipment` | Add equipment | Ongeza kifaa | draft |  |
| `catalogue.add.loan` | Add a loan listing | Ongeza mkopo kwenye orodha | draft |  |
| `catalogue.adding` | Adding… | Inaongeza… | draft |  |
| `catalogue.addSubmit` | Add to the catalogue | Ongeza kwenye orodha | draft |  |
| `catalogue.chooseCategory` | Choose a category | Chagua kundi | draft |  |
| `catalogue.code` | Code | Msimbo | draft |  |
| `catalogue.colCategory` | Category | Aina | draft | Flag: Drafter: 'Aina' (type/kind) chosen over loan 'Kategoria' as the plainest word; reviewer to confirm. |
| `catalogue.colDays` | Typical days/week | Siku za kawaida kwa wiki | draft |  |
| `catalogue.colDescription` | Description | Maelezo | draft |  |
| `catalogue.colHours` | Typical h/day | Saa za kawaida kwa siku | draft |  |
| `catalogue.colLoan` | Loan | Mkopo | draft |  |
| `catalogue.colName` | Equipment | Kifaa | draft |  |
| `catalogue.colOffered` | Offered as | Kinatolewa kwa | draft |  |
| `catalogue.colPower` | Rated power | Nguvu iliyoandikwa | draft |  |
| `catalogue.colPrice` | Indicative price | Bei ya makadirio | draft | Prices are INDICATIVE, never quotations. Must not read as a firm offer. |
| `catalogue.colRange` | Amount range | Kiwango cha kiasi | draft |  |
| `catalogue.colRent` | Rent per day | Kodi kwa siku | draft |  |
| `catalogue.createTitle.equipment` | New equipment | Kifaa kipya | draft |  |
| `catalogue.createTitle.loan` | New loan listing | Mkopo mpya kwenye orodha | draft |  |
| `catalogue.descriptionEn` | Description (English) | Maelezo (Kiingereza) | draft |  |
| `catalogue.descriptionSw` | Description (Swahili) | Maelezo (Kiswahili) | draft |  |
| `catalogue.indicativeNote` | Every figure here is indicative: not a quotation and not an offer. | Kila takwimu hapa ni elekezi: si nukuu ya bei na si ahadi. | draft | Prices are INDICATIVE, never quotations. Must not read as a firm offer. |
| `catalogue.maxInput` | Largest amount (TZS, indicative) | Kiasi cha juu (TZS, elekezi) | draft |  |
| `catalogue.minInput` | Smallest amount (TZS, indicative) | Kiasi cha chini (TZS, elekezi) | draft |  |
| `catalogue.nameEn` | Name (English) | Jina (Kiingereza) | draft |  |
| `catalogue.nameSw` | Name (Swahili) | Jina (Kiswahili) | draft |  |
| `catalogue.notANumber` | Enter a number, using a point for decimals. | Andika namba, ukitumia nukta kwa desimali. | draft |  |
| `catalogue.note` | Equipment to rent or buy, and loan listings. Prices are indicative, not quotations, and a loan listing is not an offer. | Vifaa vya kukodi au kununua, na orodha ya mikopo. Bei ni elekezi, si nukuu za bei, na mkopo ulioorodheshwa si ahadi ya mkopo. | draft |  |
| `catalogue.priceInput` | Indicative price to buy (TZS) | Bei elekezi ya kununua (TZS) | draft | Prices are INDICATIVE, never quotations. Must not read as a firm offer. Flag: Drafter: 'bei elekezi' per glossary, which warns it may read as a regulated price. Same open question as equipment.indicativePrice. |
| `catalogue.rentInput` | Indicative rent per day (TZS) | Kodi elekezi kwa siku (TZS) | draft | Flag: Drafter: 'kodi elekezi' composed from glossary 'elekezi'; 'kodi' also means tax in Swahili. Native reviewer must confirm it reads as rent. |
| `catalogue.required` | Required. | Hii ni lazima. | draft |  |
| `catalogue.title` | Resource catalogue | Orodha ya rasilimali | draft | Flag: Drafter: 'Orodha ya rasilimali' composed; 'rasilimali' unverified. |
| `coverage.available` | Available | Ugavi unaopatikana | draft |  |
| `coverage.availableNow` | Available now | Ugavi unaopatikana sasa | draft |  |
| `coverage.committed` | Already committed | Tayari umeahidiwa | draft | Flag: Drafter: 'Tayari umeahidiwa' follows glossary 'ugavi ulioahidiwa' [UNVERIFIED]. |
| `coverage.committedNote` | Committed supply is promised to a live opportunity and is not available again. | Ugavi uliowekewa ahadi kwa fursa inayoendelea hautapatikana tena. | draft | Flag: Reworded after the cross-check; unreviewed. |
| `coverage.coverageOf` | Coverage of {{total}} | Yaliyokidhiwa kati ya {{total}} | draft | Keep {{total}} |
| `coverage.demand` | Demand | Mahitaji | draft |  |
| `coverage.label` | Coverage | Yaliyokidhiwa | draft | Flag: Drafter: Coverage rendered 'Yaliyokidhiwa' (glossary label suggestion); 'Ufikiaji/wigo' avoided. [UNVERIFIED] Cross-check (low): 'Yaliyokidhiwa' is a bare relative clause ('those met'); as a standalone label it reads as a fragment and the back-translator needed a gloss. Also drops the word 'demand', so it could be read as any met quantity. |
| `coverage.notCoveredOf` | Not covered of {{total}} | Yasiyokidhiwa kati ya {{total}} | draft | Keep {{total}} |
| `demand.buyer` | Buyer | Mnunuzi | draft |  |
| `demand.chooseBuyer` | Choose a buyer | Chagua mnunuzi | draft |  |
| `demand.chooseCrop` | Choose a crop | Chagua zao | draft |  |
| `demand.colAvailable` | Available | Kinachopatikana | draft |  |
| `demand.colBuyer` | Buyer | Mnunuzi | draft |  |
| `demand.colCoverable` | Coverable | Kinachoweza kukidhi | draft | Flag: Drafter: 'Coverable' (how much of the demand a village could meet) has no glossary term; 'Kinachoweza kukidhi' is composed from the attested 'kukidhi' (coverage decision 'yaliyokidhiwa'). Cross-check (low): 'Kinachoweza kukidhi' has no object and needed a parenthetical '(the demand)' in the back-translation; unclear as a bare column header. Already flagged by the drafter. |
| `demand.colCoverage` | Coverage | Yaliyokidhiwa | draft |  |
| `demand.colCrop` | Crop | Zao | draft |  |
| `demand.colOpportunity` | Opportunity | Fursa | draft |  |
| `demand.colPrice` | Indicative price/kg | Bei ya makadirio/kg | draft | Prices are INDICATIVE, never quotations. Must not read as a firm offer. Flag: Drafter: Indicative = 'ya makadirio' per glossary decision (not 'elekezi'); reviewer to confirm. Also demand.pricePerKg. |
| `demand.colQuantity` | Quantity | Kiasi | draft |  |
| `demand.colStatus` | Status | Hali | draft |  |
| `demand.colVillage` | Village | Kijiji | draft |  |
| `demand.colWindow` | Window | Kipindi | draft |  |
| `demand.create` | Record demand | Rekodi mahitaji | draft |  |
| `demand.createOpportunity` | Create opportunity | Unda fursa | draft |  |
| `demand.createTitle` | Record a demand | Rekodi mahitaji | draft |  |
| `demand.creating` | Recording… | Inarekodi… | draft |  |
| `demand.creatingOpportunity` | Creating… | Inaunda… | draft |  |
| `demand.crop` | Crop | Zao | draft |  |
| `demand.deliveryPoint` | Delivery point | Mahali pa kuwasilisha | draft | Flag: Drafter: 'Delivery point' = 'Mahali pa kuwasilisha' (composed, unverified); 'kupokea' (receiving) is a possible alternative. |
| `demand.matches` | Villages that could supply this | Vijiji vinavyoweza kukidhi mahitaji haya | draft | Flag: Reworded after the cross-check; unreviewed. |
| `demand.noneDetail` | Record a buyer requirement above and it will appear here. | Rekodi hitaji la mnunuzi hapo juu nalo litaonekana hapa. | draft |  |
| `demand.noneTitle` | No demand recorded yet | Bado hakuna mahitaji yaliyorekodiwa | draft |  |
| `demand.noSupplyDetail` | No village has available supply of this crop in this window. That is an honest zero, not a missing row. | Hakuna kijiji chenye ugavi unaopatikana wa zao hili katika kipindi hiki. Hilo ni sifuri ya kweli, si safu iliyokosekana. | draft | Flag: Drafter: 'not a missing row' rendered as 'si safu iliyokosekana' (safu = table row); staff-facing, unverified. |
| `demand.noSupplyTitle` | No matching supply | Hakuna ugavi unaolingana | draft |  |
| `demand.notANumber` | Enter a number. | Andika namba. | draft |  |
| `demand.notASale` | An opportunity is not a sale, a delivery or a payment. It records that a village could supply a buyer. | Fursa si mauzo, si uwasilishaji wa mazao wala si malipo. Inarekodi kwamba kijiji kinaweza kumsambazia mnunuzi. | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `demand.notFoundDetail` | It may not exist, or you may not have access to it. | Huenda hayapo, au huenda huna ruhusa ya kuyaona. | draft |  |
| `demand.notFoundTitle` | Demand not found | Mahitaji hayakupatikana | draft |  |
| `demand.priceNotANumber` | Enter a number, using a point for decimals (for example 12.5). | Andika namba, ukitumia nukta kwa desimali (kwa mfano 12.5). | draft | Prices are INDICATIVE, never quotations. Must not read as a firm offer. |
| `demand.pricePerKg` | Indicative price per kg | Bei ya makadirio kwa kg | draft | Prices are INDICATIVE, never quotations. Must not read as a firm offer. |
| `demand.qualityNote` | Quality note | Maelezo ya ubora | draft |  |
| `demand.quantity` | Quantity (kg) | Kiasi (kg) | draft |  |
| `demand.required` | This is required. | Hii ni lazima. | draft |  |
| `demand.title` | Buyer demand | Mahitaji ya mnunuzi | draft |  |
| `demand.twoDecimals` | Use at most 2 decimal places. | Tumia desimali zisizozidi 2. | draft |  |
| `demand.window` | Window | Kipindi | draft |  |
| `demand.windowEnd` | Window ends | Mwisho wa kipindi | draft |  |
| `demand.windowStart` | Window starts | Mwanzo wa kipindi | draft |  |
| `demandStatus.cancelled` | Cancelled | Imeghairiwa | draft |  |
| `demandStatus.closed` | Closed | Imefungwa | draft |  |
| `demandStatus.matched` | Matched | Imeoanishwa | draft | Flag: Drafter: 'Imeoanishwa' (paired) for demand status Matched is unattested; alternative 'Imelinganishwa'. |
| `demandStatus.open` | Open | Wazi | draft |  |
| `opportunity.actionAccept` | Buyer accepted | Mnunuzi amekubali | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.actionDecline` | Buyer declined | Mnunuzi amekataa | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.actionLapse` | Mark lapsed | Weka alama kuwa imeisha muda wake | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. Flag: Drafter: 'Mark lapsed' = 'Weka imeisha muda' composed from unverified 'Imeisha muda wake'; may be worded better by a native reader. Reworded after the cross-check; unreviewed. |
| `opportunity.actionShare` | Share with buyer | Shirikisha mnunuzi | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. Flag: Drafter: 'Share with buyer' = 'Shirikisha mnunuzi' (from status 'Imeshirikishwa'); can read as 'involve the buyer'. Unverified. Cross-check (low): 'Shirikisha mnunuzi' can read as 'involve the buyer'; the blind back-translation hedged between 'involve' and 'share with'. Consistent with the existing status 'Imeshirikishwa', so left for the native reviewer. |
| `opportunity.attach` | Attach | Ambatisha | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.attachClosedDetail` | This opportunity is closed, so its supply no longer counts as committed. A line attached here would record a commitment against nothing. | Fursa hii imefungwa, kwa hiyo ugavi wake haujahesabiwa tena kuwa umeahidiwa. Mstari ulioambatishwa hapa ungerekodi ahadi isiyo na fursa hai inayoihusu. | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.attachClosedTitle` | Nothing more can be attached | Hakuna kinachoweza kuambatishwa tena | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.attachHarvest` | Available harvest | Mavuno yanayopatikana | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.attaching` | Attaching… | Inaambatisha… | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.attachKg` | Contribute (kg) | Changia (kg) | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.attachTitle` | Attach supply | Ambatisha ugavi | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.chooseHarvest` | Choose a harvest figure | Chagua kiasi cha mavuno | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.chooseHarvestRequired` | Choose a harvest figure to attach. | Chagua kiasi cha mavuno cha kuambatisha. | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.colAttachedBy` | Attached by | Kiliambatishwa na | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. Flag: Drafter: 'ambatisha' as in opportunity.attach; ki- concord follows colContributed ('Kilichochangiwa', the amount). If read as 'ugavi', it would be 'Uliambatishwa na'. Native reviewer needed. |
| `opportunity.colContributed` | Contributed | Kilichochangiwa | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.colCycle` | Crop cycle | Msimu wa zao | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.colFarmer` | Farmer | Mkulima | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.colPlot` | Plot | Kipande | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.demandQuantity` | Buyer's demand | Mahitaji ya mnunuzi | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.harvestFrom` | harvest from {{date}} | mavuno kuanzia {{date}} | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. Keep {{date}} Flag: Drafter: 'kuanzia' (starting from) is NOT in the glossary; composed. Native reviewer needed. |
| `opportunity.harvestOption` | {{expected}} expected · {{available}} available | {{expected}} yanatarajiwa · {{available}} yanapatikana | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. Keep {{expected}} {{available}} |
| `opportunity.kgDecimals` | Use at most 2 decimal places. | Tumia desimali zisizozidi 2. | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.kgMoreThanZero` | A contribution has to be more than zero. | Kiasi kinachochangiwa lazima kiwe zaidi ya sifuri. | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.kgNotANumber` | Enter a number. | Andika namba. | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.kgRequired` | Enter how many kilograms. | Andika kilo ngapi. | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.kgTooLarge` | This number is too large for the field. | Namba hii ni kubwa mno kwa sehemu hii. | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.moving` | Saving… | Inahifadhi… | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.noneAvailableDetail` | Every current harvest figure for this village, crop and window is already committed. | Kila kiasi cha sasa cha mavuno kwa kijiji hiki, zao hili na kipindi hiki tayari kimeahidiwa. | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.noneAvailableTitle` | Nothing available to attach | Hakuna cha kuambatisha | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.noSupplyDetail` | Attach available harvest figures below. | Ambatisha kiasi cha mavuno kinachopatikana hapa chini. | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.noSupplyTitle` | No supply attached yet | Bado hakuna ugavi ulioambatishwa | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.notASale` | An opportunity is not a sale, a delivery or a payment. Accepted means both sides agreed to keep talking; nothing has moved. | Fursa si mauzo, si uwasilishaji wa mazao wala si malipo. 'Imekubaliwa' maana yake pande zote mbili zimekubali kuendelea kuzungumza; hakuna kilichohamishwa. | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. Flag: Reworded after the cross-check; unreviewed. |
| `opportunity.notFoundDetail` | It may not exist, or you may not have access to it. | Huenda haipo, au huenda huna ruhusa ya kuiona. | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.notFoundTitle` | Opportunity not found | Fursa haikupatikana | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.offered` | Offered | Kilichopendekezwa | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. Flag: Drafter: 'Offered' total rendered as 'Kilichopendekezwa' (proposed), matching status 'Imependekezwa' and farmer copy; not a sale. Native reviewer to confirm. |
| `opportunity.offeredNote` | This total is re-summed by the database from the supply lines below. It cannot drift from them. | Jumla hii huhesabiwa upya na hifadhidata kutoka kwenye mistari ya ugavi iliyo hapa chini. Haiwezi kutofautiana nayo. | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. Flag: Drafter: 're-summed by the database' rendered with 'hifadhidata' (database) and 'mistari ya ugavi' (supply lines); technical terms for staff, unverified. |
| `opportunity.optionAvailable` | Available | Yanayopatikana | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. Flag: Drafter: relative form agreeing with mavuno, as in 'Mavuno yanayopatikana' (opportunity.attachHarvest); unverified. |
| `opportunity.optionExpected` | Expected | Yanayotarajiwa | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. Flag: Drafter: from glossary 'expected harvest' = 'Mavuno yanayotarajiwa' (unverified), shortened to the relative form agreeing with mavuno. |
| `opportunity.optionHarvestFrom` | Harvest from | Mavuno kuanzia | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. Flag: Drafter: 'kuanzia' (starting from) is NOT in the glossary; composed. Native reviewer needed. |
| `opportunity.releaseDetail` | {{kg}} returns to available supply for this village, and the buyer's coverage falls. This cannot be undone: a declined or lapsed opportunity cannot be reopened. | Kiasi cha {{kg}} kinarudi kwenye ugavi unaopatikana wa kijiji hiki, na sehemu ya mahitaji ya mnunuzi iliyokidhiwa inapungua. Hili haliwezi kutenduliwa: fursa ambayo mnunuzi ameikataa au iliyoisha muda haiwezi kufunguliwa tena. | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. Keep {{kg}} |
| `opportunity.releasedNote` | Closed. Its {{kg}} has gone back to available supply for this village. The supply lines below stay on the record — nothing was deleted. | Imefungwa. Kiasi chake cha {{kg}} kimerudi kwenye ugavi unaopatikana wa kijiji hiki. Mistari ya ugavi iliyo hapa chini inabaki kwenye rekodi — hakuna kilichofutwa. | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. Keep {{kg}} |
| `opportunity.releaseNo` | Cancel | Ghairi | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.releaseTitle` | Release the committed supply? | Achilia ugavi ulioahidiwa? | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.releaseYes` | Yes, release the supply | Ndiyo, achilia ugavi | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.supplyLines` | Supply | Ugavi | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `opportunity.title` | Opportunity | Fursa | draft | An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking. |
| `ops.allStatuses` | All statuses | Hali zote | draft |  |
| `ops.allVillages` | All villages | Vijiji vyote | draft |  |
| `ops.applicant` | Applicant | Mwombaji | draft |  |
| `ops.approve` | Approve | Idhinisha | draft |  |
| `ops.approvedPeak` | Approved peak | Kilele kilichoidhinishwa | draft |  |
| `ops.capacity` | Planned capacity | Uwezo uliopangwa | draft | Capacity is PLANNED or NAMEPLATE, never measured. Always shown with its basis. |
| `ops.capacityBasis` | Basis | Msingi | draft | Capacity is PLANNED or NAMEPLATE, never measured. Always shown with its basis. Flag: Drafter: 'Msingi' for basis [UNVERIFIED] per glossary (alternative 'kigezo'). |
| `ops.colAcquisition` | Rent or buy | Kukodi au kununua | draft |  |
| `ops.colApplicant` | Applicant | Mwombaji | draft |  |
| `ops.colEquipment` | Equipment | Kifaa | draft |  |
| `ops.colEstKw` | Est. kW | Makadirio ya kW | draft | Flag: Drafter: 'Makadirio kW' kept short for a column header; estimate wording preserved. Reworded after the cross-check; unreviewed. |
| `ops.colStatus` | Status | Hali | draft |  |
| `ops.colSubmitted` | Submitted | Imewasilishwa | draft |  |
| `ops.colVillage` | Village | Kijiji | draft |  |
| `ops.decisionNote` | Decision note | Maelezo ya uamuzi | draft |  |
| `ops.decisionNoteRequired` | A decision needs a note explaining it. | Uamuzi unahitaji maelezo yanayouelezea. | draft |  |
| `ops.farm` | Farm | Shamba | draft |  |
| `ops.filterStatus` | Status | Hali | draft |  |
| `ops.filterVillage` | Village | Kijiji | draft |  |
| `ops.headroom` | Village headroom | Uwezo uliobaki wa kijiji | draft | Flag: Drafter: 'Uwezo uliobaki' for headroom is [UNVERIFIED] per glossary (no Tanzanian term). |
| `ops.neverSummed` | Prospective and approved demand are separate figures and are never added together. | Mahitaji yanayotarajiwa na yaliyoidhinishwa ni takwimu tofauti na hayajumlishwi kamwe. | draft | Flag: Reworded after the cross-check; unreviewed. |
| `ops.noActions` | No actions are available in this state. | Hakuna hatua zinazopatikana katika hali hii. | draft |  |
| `ops.noHeadroom` | No capacity recorded for this village | Hakuna uwezo uliorekodiwa kwa kijiji hiki | draft |  |
| `ops.noHeadroomDetail` | A current village_capacity row is needed before headroom can be shown. | Rekodi ya sasa ya village_capacity inahitajika kabla uwezo uliobaki haujaonyeshwa. | draft | Flag: Drafter: Identifier 'village_capacity' kept as written (database table name shown to staff). |
| `ops.noRequestsDetail` | Try a different status or village. | Jaribu hali au kijiji kingine. | draft |  |
| `ops.noRequestsTitle` | No requests match | Hakuna maombi yanayolingana | draft |  |
| `ops.notFoundDetail` | It may not exist, or you may not have access to it. | Huenda halipo, au huna ruhusa ya kulifikia. | draft |  |
| `ops.notFoundTitle` | Request not found | Ombi halikupatikana | draft |  |
| `ops.prospectivePeak` | Prospective peak | Kilele kinachotarajiwa | draft | Prospective and approved demand are separate figures and are NEVER summed. Flag: Drafter: 'Kilele kinachotarajiwa' / 'Kilele kilichoidhinishwa' follow glossary 'kilele kilichokadiriwa' pattern; [UNVERIFIED] combinations. |
| `ops.reject` | Reject | Kataa | draft |  |
| `ops.requestsTitle` | Request pipeline | Mkondo wa maombi | draft | Flag: Drafter: Same composed term 'Mkondo wa maombi' for 'Request pipeline'. |
| `ops.reviewTitle` | Review request | Pitia ombi | draft |  |
| `ops.simultaneity` | Simultaneity factor | Kigezo cha matumizi ya wakati mmoja | draft | Flag: Drafter: 'Kigezo cha matumizi ya wakati mmoja' [UNVERIFIED] per glossary; needs energy-sector reviewer. Cross-check (low): Very long for a field label and column header (also villages.colSimultaneity); the blind back-translator needed a parenthetical gloss to understand it. Term is unverified; needs energy-sector reviewer. |
| `ops.snapshotted` | Snapshotted inputs | Dhana zilizohifadhiwa | draft | Flag: Drafter: 'Dhana zilizohifadhiwa' reuses existing 'Dhana' (Assumptions); avoided 'pembejeo' (farm inputs) as misleading. Cross-check (low): English 'Snapshotted inputs' says the inputs were frozen at calculation time; 'Dhana zilizohifadhiwa' reads only as 'saved assumptions'. The note below restores the timing, so acceptable. |
| `ops.snapshottedNote` | These are the assumptions as they were when the estimate was calculated. | Hizi ni dhana kama zilivyokuwa wakati makadirio yalipokokotolewa. | draft |  |
| `ops.startReview` | Start review | Anza mapitio | draft |  |
| `ops.village` | Village | Kijiji | draft |  |
| `ops.working` | Working… | Inashughulikia… | draft | Flag: Reworded after the cross-check; unreviewed. |
| `opsHome.awaitingReview` | Requests awaiting review | Maombi yanayosubiri mapitio | draft |  |
| `opsHome.awaitingReviewDetail` | Submitted or under review | Yaliyowasilishwa au yanayopitiwa | draft |  |
| `opsHome.lead` | Three queues. Every figure is a link, because a count nobody can act on is a statistic. | Orodha tatu. Kila takwimu ni kiungo, kwa sababu idadi ambayo hakuna anayeweza kuifanyia kazi ni takwimu tu. | draft | Flag: Drafter: Quip 'a count nobody can act on is a statistic' rendered literally with 'takwimu tu'; tone may need reviewer polish. |
| `opsHome.openDemands` | Open buyer demands | Mahitaji ya wanunuzi yaliyo wazi | draft |  |
| `opsHome.openDemandsDetail` | Still looking for supply | Bado yanatafuta ugavi | draft |  |
| `opsHome.openOrderBook` | Open the order book | Fungua orodha ya mahitaji ya wanunuzi | draft | Flag: Drafter: English 'order book' rendered as 'orodha ya mahitaji ya wanunuzi' to avoid 'oda'/'mauzo', which could read as sales; it opens the demand screen. |
| `opsHome.openPipeline` | Open the pipeline | Fungua mkondo wa maombi | draft | Flag: Drafter: 'Mkondo wa maombi' (pipeline) is composed; no Tanzanian source. Alternative 'Hatua za maombi'. |
| `opsHome.openVerifyQueue` | Open the verify queue | Fungua orodha ya kuhakiki | draft |  |
| `opsHome.outstandingRecords` | Records to verify | Rekodi za kuhakiki | draft |  |
| `opsHome.outstandingRecordsDetail` | Captured but not yet checked | Zimekusanywa lakini bado hazijakaguliwa | draft |  |
| `opsHome.title` | Today | Leo | draft |  |
| `surveyAdmin.addQuestion` | Add question | Ongeza swali | draft |  |
| `surveyAdmin.adminOnly` | Only an admin can author surveys. | Msimamizi pekee ndiye anayeweza kuandaa madodoso. | draft |  |
| `surveyAdmin.answerCount_one` | {{count}} answer | jibu {{count}} | draft | Keep {{count}} |
| `surveyAdmin.answerCount_other` | {{count}} answers | majibu {{count}} | draft | Keep {{count}} |
| `surveyAdmin.auditNote` | A random share of vouchers can only be redeemed by ops or admin, face to face. Nobody can tell in advance which. | Sehemu ya vocha, inayochaguliwa kwa bahati nasibu, inaweza kukabidhiwa tu na mfanyakazi wa uendeshaji au msimamizi, ana kwa ana. Hakuna anayeweza kujua mapema ni zipi. | draft | Flag: Drafter: Random selection rendered 'kwa bahati nasibu' (by chance); 'audit' = ukaguzi is unverified per glossary. |
| `surveyAdmin.auditRate` | Vouchers held for audit | Vocha zinazoshikiliwa kwa ukaguzi | draft |  |
| `surveyAdmin.average` | Average {{value}} | Wastani {{value}} | draft | Keep {{value}} |
| `surveyAdmin.close` | Close survey | Funga dodoso | draft |  |
| `surveyAdmin.closeDetail` | Farmers can no longer answer it. Vouchers already issued stay valid until they expire. | Wakulima hawawezi tena kulijibu. Vocha zilizokwisha kutolewa zinaendelea kutumika hadi ziishe muda wake. | draft |  |
| `surveyAdmin.closesOn` | Closes on | Linafungwa tarehe | draft |  |
| `surveyAdmin.closeTitle` | Close this survey? | Funga dodoso hili? | draft |  |
| `surveyAdmin.columns.outstanding` | Not yet collected | Vocha zisizochukuliwa bado | draft | Flag: Reworded after the cross-check; unreviewed. |
| `surveyAdmin.columns.redeemed` | Collected | Vocha zilizochukuliwa | draft | Flag: Drafter: Count columns use plural vocha agreement (Zilizochukuliwa / Zisizochukuliwa bado / Zilizotolewa / Zilizoisha muda / Zilizoghairiwa) without a noun; voucherStatus.* uses singular Imechukuliwa. Check they read clearly under the 'Vouchers' context. Reworded after the cross-check; unreviewed. |
| `surveyAdmin.columns.responses` | Households | Kaya | draft |  |
| `surveyAdmin.columns.reward` | Incentive | Motisha | draft |  |
| `surveyAdmin.columns.status` | Status | Hali | draft |  |
| `surveyAdmin.columns.title` | Survey | Dodoso | draft |  |
| `surveyAdmin.createdBy` | Created by {{name}} | Limeundwa na {{name}} | draft | Keep {{name}} |
| `surveyAdmin.empty` | No surveys yet. | Bado hakuna madodoso. | draft |  |
| `surveyAdmin.fields.audit_rate` | Vouchers held for audit (%) | Vocha zinazoshikiliwa kwa ukaguzi (%) | draft | Flag: Drafter: 'Vocha zinazoshikiliwa kwa ukaguzi' composed from glossary 'held for audit'; unverified. |
| `surveyAdmin.fields.closes_at` | Closes on (optional) | Linafungwa tarehe (si lazima) | draft |  |
| `surveyAdmin.fields.description_en` | Description (English) | Maelezo (Kiingereza) | draft |  |
| `surveyAdmin.fields.description_sw` | Description (Swahili) | Maelezo (Kiswahili) | draft |  |
| `surveyAdmin.fields.max_households` | Most households (optional) | Idadi ya juu ya kaya (si lazima) | draft |  |
| `surveyAdmin.fields.reward_amount` | Incentive per household (TZS) | Motisha kwa kila kaya (TZS) | draft |  |
| `surveyAdmin.fields.title_en` | Title (English) | Kichwa (Kiingereza) | draft |  |
| `surveyAdmin.fields.title_sw` | Title (Swahili) | Kichwa (Kiswahili) | draft |  |
| `surveyAdmin.from` | From | Kuanzia | draft |  |
| `surveyAdmin.goBack` | Go back | Rudi nyuma | draft |  |
| `surveyAdmin.held` | Held | Imeshikiliwa | draft |  |
| `surveyAdmin.highest` | Highest {{value}} | Juu kabisa {{value}} | draft | Keep {{value}} |
| `surveyAdmin.intro` | Surveys for farmers. Each household can answer a survey once and collects the incentive in cash at the office. | Madodoso kwa wakulima. Kila kaya inaweza kujibu dodoso mara moja na kuchukua motisha kwa fedha taslimu ofisini. | draft |  |
| `surveyAdmin.kinds.multi_choice` | Several choices | Chaguo kadhaa | draft |  |
| `surveyAdmin.kinds.number` | A number | Namba | draft | Flag: Drafter: 'A number' rendered 'Namba' (as answer type); alternative 'Nambari'. |
| `surveyAdmin.kinds.single_choice` | One choice | Chaguo moja | draft |  |
| `surveyAdmin.kinds.text` | Free text | Maandishi huru | draft | Flag: Drafter: 'Free text' = 'Maandishi huru' composed; unverified. |
| `surveyAdmin.kinds.yes_no` | Yes or no | Ndiyo au hapana | draft |  |
| `surveyAdmin.logColumns.amount` | Incentive | Motisha | draft |  |
| `surveyAdmin.logColumns.household` | Household | Kaya | draft |  |
| `surveyAdmin.logColumns.idType` | ID checked | Kitambulisho kilichokaguliwa | draft |  |
| `surveyAdmin.logColumns.officer` | Handed over by | Imekabidhiwa na | draft |  |
| `surveyAdmin.logColumns.survey` | Survey | Dodoso | draft |  |
| `surveyAdmin.logColumns.village` | Village | Kijiji | draft |  |
| `surveyAdmin.logColumns.when` | When | Lini | draft | Flag: Drafter: 'Lini' (when) as a column header; plain but unverified as UI label. |
| `surveyAdmin.lowest` | Lowest {{value}} | Chini kabisa {{value}} | draft | Keep {{value}} |
| `surveyAdmin.maxHouseholds` | Most households | Idadi ya juu ya kaya | draft |  |
| `surveyAdmin.new` | New survey | Dodoso jipya | draft |  |
| `surveyAdmin.noQuestions` | No questions yet. | Bado hakuna maswali. | draft |  |
| `surveyAdmin.noRedemptions` | Nothing handed over in these dates | Hakuna kilichokabidhiwa katika tarehe hizi | draft |  |
| `surveyAdmin.noRedemptionsDetail` | Try a wider date range. | Jaribu kipindi kirefu zaidi cha tarehe. | draft | Flag: Cross-check (low): English 'wider' range became 'kirefu zaidi' (longer), which suggests only extending the period, not widening it; small meaning shift. |
| `surveyAdmin.noVouchers` | No vouchers yet | Bado hakuna vocha | draft |  |
| `surveyAdmin.noVouchersDetail` | A voucher is issued when a household answers this survey. | Vocha hutolewa kaya inapojibu dodoso hili. | draft |  |
| `surveyAdmin.publish` | Publish | Chapisha | draft | Flag: Drafter: 'Chapisha' follows existing bundle 'Limechapishwa'; glossary has no publish row (unverified). |
| `surveyAdmin.publishDetail` | Farmers see it straight away. Once published, the questions and the incentive cannot be changed. | Wakulima wataliona mara moja. Likishachapishwa, maswali na motisha hayawezi kubadilishwa. | draft |  |
| `surveyAdmin.publishedBy` | Published {{date}} by {{name}} | Limechapishwa {{date}} na {{name}} | draft | Keep {{date}} {{name}} |
| `surveyAdmin.publishTitle` | Publish this survey? | Chapisha dodoso hili? | draft |  |
| `surveyAdmin.question.addOption` | Add option | Ongeza chaguo | draft |  |
| `surveyAdmin.question.kind` | Answer type | Aina ya jibu | draft |  |
| `surveyAdmin.question.moveDown` | Move down | Sogeza chini | draft |  |
| `surveyAdmin.question.moveUp` | Move up | Sogeza juu | draft |  |
| `surveyAdmin.question.optionLabel` | Option (English) | Chaguo (Kiingereza) | draft |  |
| `surveyAdmin.question.optionLabelSw` | Option (Swahili) | Chaguo (Kiswahili) | draft |  |
| `surveyAdmin.question.options` | Options | Chaguo | draft |  |
| `surveyAdmin.question.prompt_en` | Question (English) | Swali (Kiingereza) | draft |  |
| `surveyAdmin.question.prompt_sw` | Question (Swahili) | Swali (Kiswahili) | draft |  |
| `surveyAdmin.question.remove` | Remove | Ondoa | draft |  |
| `surveyAdmin.question.required` | Required | Lazima | draft |  |
| `surveyAdmin.questions` | Questions | Maswali | draft |  |
| `surveyAdmin.redemptions` | Redemptions | Motisha zilizokabidhiwa | draft | Flag: Drafter: 'Makabidhiano' (mutual handing over, from kabidhi) composed as a tab/heading for 'Redemptions'; not attested. Reworded after the cross-check; unreviewed. |
| `surveyAdmin.redemptionsIntro` | Every incentive handed over, for reconciling cash each day. | Kila motisha iliyokabidhiwa, kwa ajili ya kulinganisha fedha taslimu kila siku. | draft |  |
| `surveyAdmin.results` | Results | Matokeo | draft |  |
| `surveyAdmin.save` | Save draft | Hifadhi rasimu | draft |  |
| `surveyAdmin.saved` | Saved | Imehifadhiwa | draft |  |
| `surveyAdmin.saving` | Saving… | Inahifadhi… | draft |  |
| `surveyAdmin.selectVoucher` | Choose a voucher to see its audit trail. | Chagua vocha ili kuona kumbukumbu zake za ukaguzi. | draft |  |
| `surveyAdmin.summary.expired` | Expired | Vocha zilizoisha muda | draft | Flag: Reworded after the cross-check; unreviewed. |
| `surveyAdmin.summary.issued` | Issued | Vocha zilizotolewa | draft | Flag: Drafter: Bare plural-agreement tile labels (see columns.redeemed); reviewer may prefer 'Vocha zilizotolewa'. Reworded after the cross-check; unreviewed. |
| `surveyAdmin.summary.outstanding` | Not yet collected | Vocha zisizochukuliwa bado | draft | Flag: Reworded after the cross-check; unreviewed. |
| `surveyAdmin.summary.redeemed` | Collected | Vocha zilizochukuliwa | draft | Flag: Reworded after the cross-check; unreviewed. |
| `surveyAdmin.summary.responses` | Households answered | Kaya zilizojibu | draft |  |
| `surveyAdmin.summary.void` | Cancelled | Vocha zilizoghairiwa | draft | Flag: Reworded after the cross-check; unreviewed. |
| `surveyAdmin.swahiliNote` | Swahili text needs a native reviewer before farmers see it. Leave it blank to show English. | Maandishi ya Kiswahili yanapaswa kukaguliwa na mzungumzaji asilia wa Kiswahili kabla wakulima hawajayaona. Yaache wazi ili Kiingereza kionyeshwe. | draft | Flag: Drafter: 'mzungumzaji asilia wa Kiswahili' (native speaker) is composed, no Tanzanian source; reviewer may prefer another phrase. |
| `surveyAdmin.tally` | Answers | Majibu | draft |  |
| `surveyAdmin.textNotTallied` | Free-text answers are not counted here. | Majibu ya maandishi huru hayahesabiwi hapa. | draft |  |
| `surveyAdmin.title` | Surveys | Madodoso | draft |  |
| `surveyAdmin.to` | To | Hadi | draft |  |
| `surveyAdmin.totals` | Per officer per day | Kwa kila afisa kwa siku | draft | Flag: Drafter: 'Kwa kila afisa kwa siku' - 'afisa' generic (staff handing over), not 'Afisa Ugani'. Cross-check (low): Bundle uses 'mfanyakazi' for the people handing over cash (redeem.*, dbError.*) while this uses 'afisa'; ops/admin staff are not field officers. Fine as a literal of 'officer' but inconsistent in wording. |
| `surveyAdmin.totalsColumns.amount` | Total | Jumla | draft |  |
| `surveyAdmin.totalsColumns.day` | Day | Siku | draft |  |
| `surveyAdmin.totalsColumns.officer` | Handed over by | Imekabidhiwa na | draft |  |
| `surveyAdmin.totalsColumns.vouchers` | Vouchers | Vocha | draft |  |
| `surveyAdmin.void` | Cancel voucher | Ghairi vocha | draft | Flag: Drafter: 'Ghairi vocha' (cancel) chosen to match existing voucherStatus.void 'Imeghairiwa'; dbError uses 'imebatilishwa' for 'voided' - reviewer should unify. |
| `surveyAdmin.voidConfirm` | Cancel voucher | Ghairi vocha | draft |  |
| `surveyAdmin.voidDetail` | The household will not be able to collect this incentive. The reason is kept on the audit trail. | Kaya haitaweza kuchukua motisha hii. Sababu inahifadhiwa kwenye kumbukumbu za ukaguzi. | draft |  |
| `surveyAdmin.voidReason` | Reason | Sababu | draft |  |
| `surveyAdmin.voidTitle` | Cancel this voucher? | Ghairi vocha hii? | draft |  |
| `surveyAdmin.voucherColumns.amount` | Incentive | Motisha | draft |  |
| `surveyAdmin.voucherColumns.audit` | Audit | Ukaguzi | draft |  |
| `surveyAdmin.voucherColumns.household` | Household | Kaya | draft |  |
| `surveyAdmin.voucherColumns.redeemedBy` | Handed over by | Imekabidhiwa na | draft |  |
| `surveyAdmin.voucherColumns.respondent` | Answered by | Imejibiwa na | draft |  |
| `surveyAdmin.voucherColumns.status` | Status | Hali | draft |  |
| `surveyAdmin.voucherColumns.submitted` | Answered | Tarehe ya kujibu | draft | Flag: Drafter: Short 'Answered' date column rendered 'Tarehe ya kujibu' (date of answering) to avoid an ambiguous bare verb. |
| `surveyAdmin.voucherFor` | Voucher for {{household}} | Vocha ya {{household}} | draft | Keep {{household}} |
| `surveyAdmin.vouchers` | Vouchers | Vocha | draft |  |
| `tour.ops.authoringCreateBody` | New survey opens a short form for a title and the fixed incentive. Open it to look — nothing is saved until you press Save draft. | Dodoso jipya hufungua fomu fupi ya kichwa na motisha ya kiasi maalum. Ifungue uangalie — hakuna kinachohifadhiwa hadi ubonyeze Hifadhi rasimu. | draft |  |
| `tour.ops.authoringCreateTitle` | Start a new survey | Anza dodoso jipya | draft |  |
| `tour.ops.authoringDraftBody` | A draft is the only stage where the wording can change. Next opens the draft survey, so you can see how one is written. | Rasimu ndiyo hatua pekee ambapo maneno yanaweza kubadilishwa. Endelea hufungua dodoso ambalo bado ni rasimu, ili uone jinsi linavyoandikwa. | draft |  |
| `tour.ops.authoringDraftTitle` | Open the draft | Fungua rasimu | draft |  |
| `tour.ops.authoringEditorBody` | Title and description in English and Swahili, the fixed incentive per household, an optional cap on households and closing date, and the share of vouchers held for ops to hand over in person. Save draft keeps changes. | Kichwa na maelezo kwa Kiingereza na Kiswahili, motisha ya kiasi maalum kwa kila kaya, kikomo cha idadi ya kaya na tarehe ya kufungwa (vyote si lazima), na sehemu ya vocha zinazoshikiliwa ili uendeshaji uzikabidhi ana kwa ana. Hifadhi rasimu huhifadhi mabadiliko. | draft | Flag: Drafter: 'Title' of a survey rendered as 'kichwa'; 'cap on households' = 'kikomo cha idadi ya kaya'. |
| `tour.ops.authoringEditorTitle` | What the form holds | Fomu ina nini | draft |  |
| `tour.ops.authoringPublishBody` | Publish opens the survey to eligible households at once. The questions and incentive then lock, and households registered after it goes live cannot answer it. The tour never presses it. | Chapisha hufungua dodoso mara moja kwa kaya zinazostahiki. Maswali na motisha kisha haviwezi kubadilishwa tena, na kaya zilizosajiliwa baada ya kuchapishwa hazitaweza kulijibu. Ziara haibonyezi kitufe hicho kamwe. | draft | Flag: Reworded after the cross-check; unreviewed. |
| `tour.ops.authoringPublishTitle` | Publishing is one-way | Kuchapisha hakuwezi kutenguliwa | draft | Flag: Drafter: 'Publish' = Chapisha, 'publishing' = kuchapisha; no Tanzanian UI source for a survey publish button. Must match surveyAdmin.publish in o1. 'Cannot be undone' = kubatilishwa, matching the officer tour's 'kubatilisha'. Reworded after the cross-check; unreviewed. |
| `tour.ops.authoringQuestionsBody` | Each question is one choice, several choices, yes or no, a number, or free text. Mark it required and use the arrows to reorder. A choice question needs an English label on every option. | Kila swali ni chaguo moja, chaguo kadhaa, ndiyo au hapana, namba, au maandishi huru. Liweke kuwa lazima na tumia mishale kubadilisha mpangilio. Swali la chaguo linahitaji lebo ya Kiingereza kwenye kila chaguo. | draft | Flag: Drafter: 'Mark it required' = 'Liweke kuwa lazima'; 'arrows' = 'mishale'. Glossary 'ni lazima' / 'Lazima' used for required. |
| `tour.ops.authoringQuestionsTitle` | Writing the questions | Kuandika maswali | draft |  |
| `tour.ops.chapter.authoring` | Writing a survey (admin) | Kuandika dodoso (msimamizi) | draft |  |
| `tour.ops.chapter.demand` | Buyer demand and opportunities | Mahitaji ya wanunuzi na fursa | draft |  |
| `tour.ops.chapter.next` | What happens next | Kinachofuata | draft |  |
| `tour.ops.chapter.queues` | What is waiting | Kinachosubiri | draft |  |
| `tour.ops.chapter.redemptions` | Redemptions and cash counts | Makabidhiano na hesabu ya fedha taslimu | draft | Flag: Drafter: 'Makabidhiano' (noun from kabidhi) is my composition for the 'Redemptions' tab; surveysBody quotes it too. Must be matched to whatever the surveyAdmin 'Redemptions' label becomes in the o1 batch. Unverified. |
| `tour.ops.chapter.reference` | Catalogue, buyers and villages | Katalogi, wanunuzi na vijiji | draft |  |
| `tour.ops.chapter.requests` | Equipment requests | Maombi ya vifaa | draft |  |
| `tour.ops.chapter.surveys` | Surveys and vouchers | Madodoso na vocha | draft |  |
| `tour.ops.chapter.tower` | Control Tower | Control Tower | draft |  |
| `tour.ops.chapter.welcome` | Welcome | Karibu | draft |  |
| `tour.ops.demandActionsBody` | It moves from proposed to shared to accepted, and can be declined or lapse at any live point. Those two ask you to confirm, release the committed supply and cannot be undone — so the tour presses none of these. | Inasogea kutoka imependekezwa hadi imeshirikishwa hadi imekubaliwa, na inaweza kukataliwa na mnunuzi au kuisha muda wake katika hatua yoyote ambayo bado inaendelea. Hizo mbili zinakuomba uthibitishe, zinaachilia ugavi ulioahidiwa na haziwezi kutenguliwa — kwa hiyo ziara haibonyezi hata moja kati ya hizi. | draft | Flag: Drafter: Status names used inline (imependekezwa, imeshirikishwa, imekubaliwa; declined = 'kukataliwa na mnunuzi'; lapsed = 'kuisha muda wake'). 'Those two' = declined and lapsed; 'Thibitisha' meaning confirm is implied by 'uthibitishe'. |
| `tour.ops.demandActionsTitle` | Moving it along | Kuisogeza mbele | draft | Flag: Reworded after the cross-check; unreviewed. |
| `tour.ops.demandAttachBody` | Choose an expected harvest and the kilograms it contributes. Attaching commits that supply and cannot be detached, so do it live on an opportunity you made yourself. Asking for more than is available is refused, quoting the real kilograms. | Chagua mavuno yanayotarajiwa na kilo yanazochangia. Kuambatisha kunaahidi ugavi huo na hakuwezi kutenganishwa, kwa hiyo fanya hivyo papo hapo kwenye fursa uliyoiunda mwenyewe. Kuomba zaidi ya kinachopatikana kunakataliwa, na ujumbe hutaja kilo zilizopo kweli. | draft | Flag: Drafter: 'Cannot be detached' = 'hakuwezi kutenganishwa'; 'is refused, quoting the real kilograms' = 'kunakataliwa, na ujumbe hutaja kilo zilizopo kweli'. Unverified phrasing. |
| `tour.ops.demandAttachTitle` | Attach supply from a harvest | Ambatisha ugavi kutoka kwenye mavuno | draft |  |
| `tour.ops.demandBody` | What buyers are asking for, and how much of it a village could cover. Prices are indicative throughout. Creating an opportunity records a match — it is not a sale. | Kile wanunuzi wanachotaka, na kiasi ambacho kijiji kinaweza kukidhi. Bei zote ni za makadirio, si nukuu ya bei. Kuunda fursa kunarekodi kwamba mnunuzi amelinganishwa na kijiji — si uuzaji. | draft | Flag: Drafter: Adds 'si nukuu ya bei' after 'Bei zote ni za makadirio' per glossary decision (indicative = bei ya makadirio, always with si nukuu ya bei). English only says 'indicative'. Also 'a match' rendered as a clause (mnunuzi amelinganishwa na kijiji) because no plain noun for 'match' exists in the glossary. |
| `tour.ops.demandCoverageBody` | Available now is expected harvest not yet promised; not covered is what the demand still lacks. Committed supply is promised to a live opportunity and sits below the line — it is never added to the bar. | Kinachopatikana sasa ni mavuno yanayotarajiwa ambayo bado hayajaahidiwa; kisichokidhiwa ni kile ambacho mahitaji bado yanakosa. Ugavi ulioahidiwa umeahidiwa kwa fursa inayoendelea na uko chini ya mstari — haujumlishwi kamwe kwenye upau. | draft | Flag: Drafter: Legend labels 'Available now' / 'Not covered' rendered as 'Kinachopatikana sasa' / 'kisichokidhiwa' as in-sentence phrases, not the exact coverage.* labels, which are not yet translated; align once they are. 'Committed supply' = 'Ugavi ulioahidiwa' (glossary, unverified). 'Live opportunity' = 'fursa inayoendelea'. |
| `tour.ops.demandCoverageTitle` | Coverage, and what is promised | Mahitaji yaliyokidhiwa, na kilichoahidiwa | draft |  |
| `tour.ops.demandCreateBody` | This opens a short form: buyer, crop, quantity, delivery window and an indicative price. Nothing is saved until you press Record demand. | Hii inafungua fomu fupi: mnunuzi, zao, kiasi, kipindi cha uwasilishaji na bei ya makadirio. Hakuna kinachohifadhiwa hadi ubonyeze Rekodi mahitaji. | draft | Flag: Drafter: Quotes button 'Record demand' as 'Rekodi mahitaji' (demand.create not yet translated). 'Nothing is saved until you press' kept as strong as English. |
| `tour.ops.demandCreateTitle` | Record what a buyer needs | Rekodi mahitaji ya mnunuzi | draft |  |
| `tour.ops.demandDetailBody` | The crop, quantity, delivery window and an indicative price — indicative, not a quotation. Below, the screen compares it with the harvest that villages expect in that window. | Zao, kiasi, kipindi cha uwasilishaji na bei ya makadirio — ya makadirio, si nukuu ya bei. Chini yake, skrini inalinganisha hitaji hilo na mavuno ambayo vijiji vinatarajia katika kipindi hicho. | draft | Flag: Reworded after the cross-check; unreviewed. |
| `tour.ops.demandDetailTitle` | One buyer's request | Hitaji la mnunuzi mmoja | draft | Flag: Drafter: Uses 'hitaji' (singular of mahitaji) to avoid clashing with 'ombi' (equipment request). |
| `tour.ops.demandMatchesBody` | Each village with expected harvest in the window: what is available, how much of the demand it could cover, and the coverage. It is a comparison — nothing is allocated. Create opportunity, where offered, records a match, not a sale. | Kila kijiji chenye mavuno yanayotarajiwa katika kipindi hicho: kinachopatikana, kiasi cha mahitaji ambacho kinaweza kukidhi, na asilimia ya mahitaji yaliyokidhiwa. Ni ulinganisho tu — hakuna kinachogawiwa. Unda fursa, inapotolewa, inarekodi kwamba mnunuzi amelinganishwa na kijiji, si uuzaji. | draft | Flag: Drafter: Quotes 'Create opportunity' as 'Unda fursa' (demand.createOpportunity not yet translated). 'Coverage' spelled out as 'asilimia ya mahitaji yaliyokidhiwa' per glossary (unverified). |
| `tour.ops.demandMatchesTitle` | Village by village | Kijiji kwa kijiji | draft |  |
| `tour.ops.demandOpportunityBody` | It records that this village could supply this buyer, nothing more. Accepted only means both sides agreed to keep talking — no sale, delivery or payment has happened. | Inarekodi kwamba kijiji hiki kinaweza kumpa mnunuzi huyu mazao, si zaidi. Imekubaliwa inamaanisha tu pande zote mbili zimekubali kuendelea kuzungumza — hakuna uuzaji, uwasilishaji wala malipo uliofanyika. | draft |  |
| `tour.ops.demandOpportunityLinkBody` | Where a village has been matched, its status links to the opportunity. An opportunity records that a village could supply a buyer — it is not a sale, a delivery or a payment. | Kijiji kilipolinganishwa na mnunuzi, hali yake ni kiungo cha fursa. Fursa inarekodi kwamba kijiji kinaweza kumpa mnunuzi mazao — si uuzaji, si uwasilishaji wala malipo. | draft |  |
| `tour.ops.demandOpportunityLinkTitle` | Open the opportunity | Fungua fursa | draft |  |
| `tour.ops.demandOpportunityTitle` | An opportunity is not a sale | Fursa si uuzaji | draft |  |
| `tour.ops.demandQuantitiesBody` | The buyer's demand and this village's offer, kept apart. The offered total is re-added by the database from the supply lines below, so the two can never drift. | Mahitaji ya mnunuzi na kiasi ambacho kijiji hiki kinaweza kutoa, vikiwa tofauti. Jumla inayoweza kutolewa inajumlishwa upya na hifadhidata kutoka kwenye mistari ya ugavi iliyo hapa chini, kwa hiyo hizo mbili haziwezi kupishana kamwe. | draft | Flag: Drafter: 'Offered' rendered as 'kilichotolewa' / 'kiasi ambacho kijiji hiki kinatoa'; ops label 'Offered' not yet translated. 'Never drift' = 'haziwezi kupishana kamwe'. Reworded after the cross-check; unreviewed. |
| `tour.ops.demandQuantitiesTitle` | Asked for, and offered | Kilichoombwa, na kinachoweza kutolewa | draft | Flag: Reworded after the cross-check; unreviewed. |
| `tour.ops.demandSupplyLinesBody` | Each line names the farmer, plot and crop cycle behind it, and links to that record. This is how a headline figure traces back to real records. | Kila mstari unataja mkulima, kipande cha shamba na msimu wa zao nyuma yake, na unaunganisha na rekodi hiyo. Hivi ndivyo takwimu kuu inavyofuatiliwa hadi rekodi zenyewe. | draft |  |
| `tour.ops.demandSupplyLinesTitle` | Every kilogram has a source | Kila kilo ina chanzo | draft |  |
| `tour.ops.demandTitle` | Buyer demand | Mahitaji ya wanunuzi | draft |  |
| `tour.ops.nextBody` | What the officer and the farmer did in this demo shows up in these screens with names beside it: records, requests, survey answers and each voucher's trail. Admin publishes surveys; ops reads the trail. | Kilichofanywa na afisa na mkulima katika onyesho hili kinaonekana kwenye skrini hizi pamoja na majina: rekodi, maombi, majibu ya madodoso na kumbukumbu ya kila vocha. Msimamizi huchapisha madodoso; uendeshaji husoma kumbukumbu. | draft |  |
| `tour.ops.nextTitle` | Everything is visible here | Kila kitu kinaonekana hapa | draft |  |
| `tour.ops.queueBody` | Each tile counts one queue and opens it. They are counts of work, not performance figures. | Kila kisanduku kinahesabu foleni moja na kinaifungua. Ni idadi ya kazi, si takwimu za utendaji. | draft | Flag: Drafter: 'Tile' rendered as 'kisanduku' (box); no glossary term. 'Performance figures' = 'takwimu za utendaji'. Reworded after the cross-check; unreviewed. |
| `tour.ops.queuesDemandsBody` | Counts the buyer demands that still need supply, and opens the order book where each one is compared with what villages could cover. | Inahesabu mahitaji ya wanunuzi ambayo bado yanahitaji ugavi, na inafungua orodha ya mahitaji ambamo kila moja linalinganishwa na kiasi ambacho vijiji vinaweza kukidhi. | draft |  |
| `tour.ops.queuesDemandsTitle` | Buyer demand still open | Mahitaji ya wanunuzi yaliyo wazi | draft |  |
| `tour.ops.queuesVerificationBody` | Records captured in the field that nobody has verified yet. It opens the officers' verify queue. Verifying cannot be undone, and a household is checked by someone other than the officer who registered it. | Rekodi zilizokusanywa uwandani ambazo bado hakuna aliyezihakiki. Inafungua foleni ya kuhakiki ya maafisa. Kuhakiki hakuwezi kutenguliwa, na kaya inahakikiwa na mtu mwingine, si afisa aliyeisajili. | draft |  |
| `tour.ops.queuesVerificationTitle` | Records waiting to be checked | Rekodi zinazosubiri kuhakikiwa | draft |  |
| `tour.ops.queueTitle` | What is waiting for you | Kinachokusubiri | draft |  |
| `tour.ops.redemptionsBody` | Every incentive handed over at the office in a range of dates, for checking each officer's cash against what they recorded. It shows the last seven days until you change it. | Kila motisha iliyokabidhiwa ofisini katika kipindi cha tarehe, ili kukagua fedha taslimu za kila afisa dhidi ya alichorekodi. Huonyesha siku saba zilizopita hadi ubadilishe. | draft |  |
| `tour.ops.redemptionsLogBody` | One row per redemption: when, who handed it over, the village, household and survey, the incentive, and which ID type was checked. The ID number is never kept. | Safu moja kwa kila vocha iliyokabidhiwa: lini, nani alikabidhi, kijiji, kaya na dodoso, motisha, na aina ya kitambulisho kilichoangaliwa. Namba ya kitambulisho haihifadhiwi kamwe. | draft |  |
| `tour.ops.redemptionsLogTitle` | Every hand-over, in full | Kila motisha iliyokabidhiwa, kwa kina | draft | Flag: Reworded after the cross-check; unreviewed. |
| `tour.ops.redemptionsRangeBody` | Set From and To to look at another range. The dates are held in the address, so a view can be shared or reloaded. Looking changes nothing. | Weka Kuanzia na Hadi ili kuona kipindi kingine. Tarehe zinashikiliwa kwenye anwani ya ukurasa, kwa hiyo mwonekano unaweza kutumwa kwa wengine au kupakiwa upya. Kuangalia hakubadilishi chochote. | draft | Flag: Drafter: From / To labels rendered as 'Kuanzia' / 'Hadi'; must match surveyAdmin.from / surveyAdmin.to in the o1 batch. 'held in the address' = 'zinashikiliwa kwenye anwani ya ukurasa' (URL), wording unverified. |
| `tour.ops.redemptionsRangeTitle` | Change the dates | Badilisha tarehe | draft |  |
| `tour.ops.redemptionsTitle` | Cash handed over, by day | Fedha taslimu zilizokabidhiwa, kwa siku | draft |  |
| `tour.ops.redemptionsTotalsBody` | How many vouchers each officer handed over on each day, and the total cash. This is the figure their cash count is checked against. | Vocha ngapi kila afisa alikabidhi kila siku, na jumla ya fedha taslimu. Hiki ndicho kiasi kinacholinganishwa na hesabu yake ya fedha. | draft | Flag: Drafter: 'their cash count is checked against' = 'inalinganishwa na hesabu yake ya fedha'; wording composed. |
| `tour.ops.redemptionsTotalsTitle` | Per officer, per day | Kwa kila afisa, kwa siku | draft |  |
| `tour.ops.referenceBasisBody` | Planned or nameplate — never measured. A village peak is not the sum of rated power: the simultaneity factor allows for equipment not all running at once. | Uliopangwa au kulingana na kibao cha mashine — haujapimwa kamwe. Kilele cha kijiji si jumla ya nguvu iliyoandikwa: kigezo cha matumizi ya wakati mmoja huzingatia kwamba vifaa havifanyi kazi vyote kwa wakati mmoja. | draft | Flag: Drafter: 'nameplate' = 'kibao cha mashine' (glossary, unverified); 'village peak' = 'kilele cha kijiji' (composed from glossary 'kilele kilichokadiriwa'); 'basis' = 'msingi' (unverified). |
| `tour.ops.referenceBasisTitle` | The basis, beside every figure | Msingi, kando ya kila kiasi | draft |  |
| `tour.ops.referenceBuyerCreateBody` | Try it: press the button and the form opens below the table. Nothing is saved until you press Add buyer, and a buyer needs only a name. | Jaribu: bonyeza kitufe na fomu itafunguka chini ya jedwali. Hakuna kinachohifadhiwa hadi ubonyeze Ongeza mnunuzi, na mnunuzi anahitaji jina tu. | draft |  |
| `tour.ops.referenceBuyerCreateTitle` | Add a buyer | Ongeza mnunuzi | draft |  |
| `tour.ops.referenceBuyersBody` | The buyers whose demand you record. Channel is only a label for how a buyer was reached — AFM means that origin and nothing more: no integration and no partnership. | Wanunuzi ambao mahitaji yao unarekodi. Njia ni lebo tu ya jinsi mnunuzi alivyofikiwa — AFM inamaanisha asili hiyo tu: hakuna muunganisho wa mifumo wala ushirikiano. | draft | Flag: Drafter: 'Channel' (column) rendered as 'Njia'; no sw label exists yet for buyers.colChannel. Must match whatever o3 batch uses. 'no integration and no partnership' = 'hakuna muunganisho wa mifumo wala ushirikiano' (unverified). |
| `tour.ops.referenceBuyersTitle` | Buyers | Wanunuzi | draft |  |
| `tour.ops.referenceCatalogueBody` | What the programme offers farmers: equipment to rent, to buy, or both, with rated power, typical hours, an indicative price and an indicative rent per day — indicative, never a quotation. A request's estimate uses the rated power listed. | Kile programu inachowapa wakulima: vifaa vya kukodi, kununua, au vyote viwili, pamoja na nguvu iliyoandikwa, saa za kawaida, bei elekezi na kodi elekezi kwa siku. Ni elekezi, si nukuu ya bei. Makadirio ya ombi yanatumia nguvu iliyoandikwa hapa. | draft |  |
| `tour.ops.referenceCatalogueCreateBody` | Try it: press the button and a short form opens below — equipment or a loan listing, by the tab you are on. Nothing is saved until you press Add to the catalogue. | Jaribu: bonyeza kitufe na fomu fupi itafunguka chini, ya kifaa au ya mkopo, kulingana na kichupo ulichopo. Hakuna kinachohifadhiwa mpaka ubonyeze Ongeza kwenye orodha. | draft |  |
| `tour.ops.referenceCatalogueCreateTitle` | Add to the catalogue | Ongeza kwenye orodha | draft |  |
| `tour.ops.referenceCatalogueTitle` | The resource catalogue | Orodha ya rasilimali | draft |  |
| `tour.ops.referenceLoansBody` | Loans are listed here too: a name, a description and an indicative range. A listing is not an offer and carries no terms; farmers ask at the office. | Mikopo pia imeorodheshwa hapa: jina, maelezo na kiwango elekezi cha kiasi. Mkopo ulioorodheshwa si ahadi na hauna masharti; wakulima huuliza ofisini. | draft |  |
| `tour.ops.referenceLoansTitle` | Loan listings | Orodha ya mikopo | draft |  |
| `tour.ops.referenceVillagesBody` | Each village's planned capacity, its simultaneity factor and the date it took effect. Planned, never measured: the basis is shown inside the capacity cell itself. | Uwezo uliopangwa wa kila kijiji, kigezo chake cha matumizi ya wakati mmoja na tarehe ya kuanza kutumika. Umepangwa, haujapimwa kamwe: msingi unaonyeshwa ndani ya kisanduku cha uwezo chenyewe. | draft | Flag: Drafter: 'simultaneity factor' = 'kigezo cha matumizi ya wakati mmoja' (glossary, unverified); 'date it took effect' = 'tarehe ya kuanza kutumika'. |
| `tour.ops.referenceVillagesTitle` | Village capacity | Uwezo wa kijiji | draft |  |
| `tour.ops.requestsBody` | Every equipment request, filterable by status and village. Opening one shows the farmer's own words, the energy estimate and the decision controls. | Kila ombi la vifaa, linaloweza kuchujwa kwa hali na kijiji. Ukilifungua unaona maneno ya mkulima mwenyewe, makadirio ya nishati na vidhibiti vya uamuzi. | draft |  |
| `tour.ops.requestsCapacityBody` | The village's capacity is a planned figure, never a measured one, and the basis it was planned on sits right beside it. | Uwezo wa kijiji ni kiasi kilichopangwa, kamwe si kilichopimwa, na msingi wa mpango huo uko papo hapo kando yake. | draft | Capacity is PLANNED or NAMEPLATE, never measured. Always shown with its basis. Flag: Drafter: Basis = 'msingi' (UNVERIFIED glossary). Kept 'planned, never measured' as strong as English. |
| `tour.ops.requestsCapacityTitle` | Capacity is planned | Uwezo umepangwa | draft | Capacity is PLANNED or NAMEPLATE, never measured. Always shown with its basis. |
| `tour.ops.requestsDecisionBody` | Start review moves a submitted request into review; from there Approve or Reject each need a written note. A decision is final and cannot be undone, so the tour never presses these — do it live on a fresh request. | Anza mapitio inahamisha ombi lililowasilishwa hadi hatua ya kupitiwa; kutoka hapo, kila kitufe cha Idhinisha na Kataa kinahitaji maelezo ya maandishi. Uamuzi ni wa mwisho na hauwezi kutenguliwa, kwa hiyo ziara haibonyezi vitufe hivi — fanya hivyo papo hapo kwenye ombi jipya. | draft | Flag: Drafter: Quotes buttons Start review / Approve / Reject as 'Anza mapitio' / 'Idhinisha' / 'Kataa' (ops.startReview etc. not yet in the bundle). Reject = 'Kataa' although request status is 'Imekataliwa'; confirm. 'Kila kimoja' agrees with 'kitufe' implicitly, may read stiffly. Reworded after the cross-check; unreviewed. |
| `tour.ops.requestsDecisionTitle` | Deciding a request | Kuamua ombi | draft |  |
| `tour.ops.requestsEstimateBody` | An estimate, not a measurement: rated power times quantity, using the hours and days the farmer gave. These are the assumptions as they stood when it was calculated, so a later catalogue change does not rewrite them. | Makadirio, si kipimo: nguvu iliyoandikwa mara idadi, kwa kutumia saa na siku alizotoa mkulima. Hizi ni dhana kama zilivyokuwa wakati makadirio yalipokokotolewa, kwa hiyo mabadiliko ya katalogi baadaye hayazibadilishi. | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. |
| `tour.ops.requestsEstimateTitle` | The farmer's energy estimate | Makadirio ya nishati ya mkulima | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. |
| `tour.ops.requestsFiltersBody` | Try it: choose a status or a village and the list narrows to match. Under review means waiting for a decision. If the list goes empty, choose All statuses again before you go on. | Jaribu: chagua hali au kijiji, na orodha inapungua kulingana na uchaguzi wako. Inapitiwa inamaanisha inasubiri uamuzi. Orodha ikibaki tupu, chagua Hali zote tena kabla ya kuendelea. | draft | Flag: Drafter: Quotes 'Under review' as 'Inapitiwa' (existing status label) and 'All statuses' as 'Hali zote' (ops.allStatuses is not yet in the Swahili bundle); confirm the final label matches. |
| `tour.ops.requestsFiltersTitle` | Filter the pipeline | Chuja mtiririko | draft |  |
| `tour.ops.requestsHeadroomBody` | Planned capacity minus the approved peak, for the whole village. It changes only when a request is approved, so it shows what is left after decisions already made. | Uwezo uliopangwa ukiondoa kilele kilichoidhinishwa, kwa kijiji kizima. Hubadilika tu ombi linapoidhinishwa, kwa hiyo unaonyesha kilichobaki baada ya maamuzi yaliyokwisha kufanywa. | draft | Flag: Drafter: Headroom = 'uwezo uliobaki', approved peak = 'kilele kilichoidhinishwa' (both UNVERIFIED glossary compositions). |
| `tour.ops.requestsHeadroomTitle` | Headroom left in the village | Uwezo uliobaki katika kijiji | draft |  |
| `tour.ops.requestsPeaksBody` | Prospective is what applicants have asked for and is still waiting; approved is what has been decided. They are never added together, and neither is a measured load. Each has the simultaneity factor applied. | Yanayotarajiwa ni yale ambayo waombaji wameomba na bado yanasubiri; yaliyoidhinishwa ni yale yaliyokwisha kuamuliwa. Hayajumlishwi kamwe, na yote mawili si mzigo uliopimwa. Kwa yote mawili kigezo cha matumizi ya wakati mmoja kimetumika. | draft | Flag: Drafter: Prospective/approved demand = 'yanayotarajiwa' / 'yaliyoidhinishwa' (glossary UNVERIFIED). Simultaneity factor = 'kigezo cha matumizi ya wakati mmoja' (UNVERIFIED, no source). 'Neither is a measured load' = 'yote mawili si mzigo uliopimwa'. |
| `tour.ops.requestsPeaksTitle` | Prospective and approved stay apart | Yanayotarajiwa na yaliyoidhinishwa hubaki tofauti | draft |  |
| `tour.ops.requestsTitle` | The request pipeline | Mtiririko wa maombi | draft | Flag: Drafter: 'Pipeline' has no glossary term; 'Mtiririko' (flow) chosen, used in requestsFiltersTitle too. Must match the ops.requestsTitle 'Request pipeline' label when drafted. |
| `tour.ops.surveysBody` | Admin writes and publishes surveys here. Each household answers once and collects a fixed cash incentive at the office. You see who answered, which vouchers are collected, and the full trail of names behind each one; Redemptions totals the cash handed over per officer per day. | Msimamizi anaandika na kuchapisha madodoso hapa. Kila kaya hujibu mara moja na kuchukua motisha ya kiasi maalum cha fedha taslimu katika ofisi. Unaona nani amejibu, vocha zipi zimechukuliwa, na kumbukumbu kamili ya majina yaliyo nyuma ya kila vocha; Makabidhiano yanajumlisha fedha taslimu zilizokabidhiwa kwa kila afisa kwa siku. | draft | Flag: Drafter: Quotes the 'Redemptions' tab as 'Makabidhiano' (unverified, see chapter.redemptions). 'Trail of names' = 'kumbukumbu kamili ya majina', reusing kumbukumbu (audit trail) wording; unverified. |
| `tour.ops.surveysListBody` | One row per survey: draft, live or closed, the fixed incentive, and how many households answered, collected it and have not yet collected. Next opens a live survey that has answers. | Safu moja kwa kila dodoso: rasimu, linaendelea au limefungwa, motisha ya kiasi maalum, na kaya ngapi zilijibu, zilichukua na hazijachukua bado. Endelea hufungua dodoso linaloendelea lenye majibu. | draft | Flag: Reworded after the cross-check; unreviewed. |
| `tour.ops.surveysListTitle` | Every survey and its counts | Kila dodoso na hesabu zake | draft |  |
| `tour.ops.surveysSummaryBody` | Households that answered, vouchers issued, collected at the office and not yet collected, plus expired and cancelled. Each figure is counted by the database — nothing is added up on this screen. | Kaya zilizojibu, vocha zilizotolewa, zilizochukuliwa ofisini na ambazo hazijachukuliwa bado, pamoja na zilizoisha muda wake na zilizoghairiwa. Kila kiasi kinahesabiwa na hifadhidata — hakuna kinachojumlishwa kwenye skrini hii. | draft |  |
| `tour.ops.surveysSummaryTitle` | The survey at a glance | Dodoso kwa mtazamo mmoja | draft |  |
| `tour.ops.surveysTallyBody` | Each question with its answers counted: how many chose each option, and the average, lowest and highest for a number. Free-text answers are not counted here. | Kila swali likiwa na majibu yake yaliyohesabiwa: kaya ngapi zilichagua kila chaguo, na wastani, kiwango cha chini na cha juu kwa namba. Majibu ya maandishi huru hayahesabiwi hapa. | draft |  |
| `tour.ops.surveysTallyTitle` | What households answered | Kaya zilijibu nini | draft |  |
| `tour.ops.surveysTitle` | Surveys and their incentives | Madodoso na motisha zake | draft |  |
| `tour.ops.surveysTrailBody` | Who registered and verified the household, who created the login, then when the survey was published, answered and the voucher issued. A collected voucher adds who handed it over and the ID type seen — never the number. | Nani alisajili na kuhakiki kaya, nani aliyeunda akaunti ya programu, kisha lini dodoso lilipochapishwa, lilipojibiwa na vocha ilipotolewa. Vocha iliyochukuliwa huongeza nani aliyeikabidhi na aina ya kitambulisho kilichoonekana — kamwe si namba. | draft | Flag: Drafter: 'created the login' = 'aliyeunda akaunti ya kuingia' (composed; no sourced term). Trail = kumbukumbu. Reworded after the cross-check; unreviewed. |
| `tour.ops.surveysTrailTitle` | Every line has a name | Kila mstari una jina | draft |  |
| `tour.ops.surveysVoidBody` | Ops and admin can cancel a voucher nobody has collected, after giving a reason that goes on its trail. It cannot be undone, so the tour never presses it. | Mfanyakazi wa uendeshaji na msimamizi wanaweza kughairi vocha ambayo hakuna aliyeichukua, baada ya kutoa sababu inayowekwa kwenye kumbukumbu zake za ukaguzi. Haiwezi kutenguliwa, kwa hiyo ziara haibonyezi kitufe hicho kamwe. | draft | Flag: Reworded after the cross-check; unreviewed. |
| `tour.ops.surveysVoidTitle` | Cancelling a voucher | Kughairi vocha | draft |  |
| `tour.ops.surveysVouchersBody` | One row per household: who answered, when, the voucher's status and the incentive. A Held mark means only ops or admin can hand it over, chosen at random. Click a row to open its trail. | Safu moja kwa kila kaya: aliyejibu, lini, hali ya vocha na motisha. Alama ya Imeshikiliwa inamaanisha ni mfanyakazi wa uendeshaji au msimamizi tu anayeweza kuikabidhi, na ilichaguliwa kwa nasibu. Bonyeza safu ili kufungua kumbukumbu zake za ukaguzi. | draft | Flag: Drafter: 'Held' badge = 'Imeshikiliwa' (glossary: held for audit); 'ops or admin' = uendeshaji au msimamizi (role terms unverified in glossary); 'trail' = kumbukumbu (glossary: kumbukumbu za ukaguzi, unverified). Reworded after the cross-check; unreviewed. |
| `tour.ops.surveysVouchersTitle` | Click a voucher | Bonyeza vocha | draft |  |
| `tour.ops.towerApprovedBody` | The estimated peak of requests decided yes, with the simultaneity factor applied. It is an estimate, not measured consumption, and it stays apart from the prospective figure. | Kilele kilichokadiriwa cha maombi yaliyoamuliwa kuwa ndiyo, kikitumia kigezo cha matumizi ya wakati mmoja. Ni makadirio, si matumizi yaliyopimwa, na kinabaki tofauti na kiasi kinachotarajiwa. | draft |  |
| `tour.ops.towerApprovedTitle` | Approved peak: decisions | Kilele kilichoidhinishwa: maamuzi | draft |  |
| `tour.ops.towerBody` | Production, energy, market and data quality for one village. Every figure is read from the database and drills down to the record it came from. | Uzalishaji, nishati, soko na ubora wa taarifa kwa kijiji kimoja. Kila takwimu inasomwa kutoka kwenye hifadhidata na inaweza kufunguliwa hadi kwenye rekodi ilikotoka. | draft | Flag: Drafter: 'Data quality' = 'ubora wa taarifa' (no glossary term); 'database' = 'hifadhidata' (standard, unverified). Reworded after the cross-check; unreviewed. |
| `tour.ops.towerCapacityBody` | What the village's power supply is planned to carry, and the basis that figure rests on. It is planned, never measured — the tag beside it says so. | Kile ambacho umeme wa kijiji umepangwa kubeba, na msingi ambao kiasi hicho kinategemea. Kimepangwa, hakijapimwa kamwe — lebo iliyo kando yake inasema hivyo. | draft | Capacity is PLANNED or NAMEPLATE, never measured. Always shown with its basis. Flag: Reworded after the cross-check; unreviewed. |
| `tour.ops.towerCapacityTitle` | Planned capacity, with its basis | Uwezo uliopangwa, na msingi wake | draft | Capacity is PLANNED or NAMEPLATE, never measured. Always shown with its basis. |
| `tour.ops.towerDrillBody` | The crop and the farmer's name in each row are links. Follow either to the record the officer wrote, with its source and whether it is verified. Every headline can be traced this way. | Zao na jina la mkulima katika kila safu ni viungo. Fuata kimojawapo hadi rekodi aliyoandika afisa, ikiwa na chanzo chake na kama imehakikiwa. Kila kiasi kikuu kinaweza kufuatiliwa hivi. | draft |  |
| `tour.ops.towerDrillTitle` | Down to the farmer's record | Hadi rekodi ya mkulima | draft |  |
| `tour.ops.towerHeadroomBody` | Planned capacity less the approved peak: what is still free. Prospective requests are not on this bar, because an application is not a load. | Uwezo uliopangwa ukiondoa kilele kilichoidhinishwa: kinachobaki. Maombi yanayotarajiwa hayamo kwenye upau huu, kwa sababu ombi si mzigo. | draft |  |
| `tour.ops.towerHeadroomTitle` | Headroom left | Uwezo uliobaki | draft |  |
| `tour.ops.towerMarketBody` | Open buyer demand against the expected harvest that is still available, shown as coverage. Available means not yet committed to an opportunity; it is an estimate, not stock in a store. | Mahitaji ya wanunuzi yaliyo wazi dhidi ya mavuno yanayotarajiwa ambayo bado yanapatikana, yakionyeshwa kama asilimia iliyokidhiwa. Kinachopatikana ni ugavi ambao bado haujaahidiwa kwa fursa; ni makadirio, si akiba iliyo ghalani. | draft | Flag: Drafter: Coverage rendered as 'asilimia iliyokidhiwa' (glossary: yaliyokidhiwa, unverified); 'stock in a store' = 'akiba iliyo ghalani'. |
| `tour.ops.towerMarketTitle` | Market: demand against supply | Soko: mahitaji dhidi ya ugavi | draft |  |
| `tour.ops.towerProductionBody` | Expected harvest, an estimate, against actual by crop and window, with planted area across cycles — not land area. Next opens See the records to show the rows behind this tile. | Mavuno yanayotarajiwa, ambayo ni makadirio, dhidi ya halisi kwa zao na kipindi, pamoja na jumla ya eneo lililopandwa katika mazao yote — si eneo la ardhi. Endelea hufungua Tazama rekodi ili kuonyesha safu zilizo nyuma ya kisanduku hiki. | draft | Flag: Drafter: 'tile' rendered as 'kisanduku' (box); no sourced dashboard term. 'See the records' = 'Tazama rekodi', taken from tower.drill (needs the o2 sw label to match). 'not land area' = 'si eneo la ardhi' as a required negation. |
| `tour.ops.towerProductionTableBody` | Each cycle behind the tile: crop, farmer, plot, harvest window, planted area, expected and actual weight. The last row is the figure on the tile, so the headline and its rows visibly match. | Kila msimu wa zao ulio nyuma ya kisanduku: zao, mkulima, kipande cha shamba, kipindi cha kuvuna, eneo lililopandwa, uzito unaotarajiwa na halisi. Safu ya mwisho ni kiasi kilicho kwenye kisanduku, ili kiasi kikuu na safu zake zionekane wazi kuwa zinalingana. | draft | Flag: Drafter: 'the headline' = 'kiasi kikuu' (main figure); composed. Reworded after the cross-check; unreviewed. |
| `tour.ops.towerProductionTableTitle` | One row per crop cycle | Safu moja kwa kila msimu wa zao | draft |  |
| `tour.ops.towerProductionTitle` | Production, then the records | Uzalishaji, kisha rekodi | draft |  |
| `tour.ops.towerProspectiveBody` | The estimated peak of requests that are submitted or under review. It is an application, not a load, and it is never added to the approved figure. | Kilele kilichokadiriwa cha maombi yaliyowasilishwa au yanayopitiwa. Ni ombi, si mzigo, na hakijumlishwi kamwe na kiasi kilichoidhinishwa. | draft | Flag: Drafter: 'load' = 'mzigo' (electrical load), plain word, unverified in this sense. 'prospective' = 'kinachotarajiwa' per glossary (unverified). |
| `tour.ops.towerProspectiveTitle` | Prospective peak: applications | Kilele kinachotarajiwa: maombi | draft |  |
| `tour.ops.towerPueBody` | Requests counted by status, with the indicative catalogue value they represent. Each status opens its requests for this village. Indicative means price times quantity — not a quotation, not financed value. | Maombi yaliyohesabiwa kwa hali, pamoja na thamani ya katalogi ya makadirio yanayowakilisha. Kila hali hufungua maombi yake ya kijiji hiki. Ya makadirio maana yake ni bei mara idadi — si nukuu ya bei, si thamani iliyofadhiliwa. | draft | Flag: Drafter: 'financed value' = 'thamani iliyofadhiliwa kwa mkopo' (composed). 'Indicative' = 'ya makadirio' per glossary decision; 'si nukuu ya bei' kept. Reworded after the cross-check; unreviewed. |
| `tour.ops.towerPueTitle` | Equipment pipeline | Mtiririko wa vifaa | draft | Flag: Drafter: 'pipeline' = 'mtiririko' (flow); no sourced term. Ops request pipeline label in o3 must agree. |
| `tour.ops.towerQualityBody` | How much of what has been recorded has been checked: persons verified, farms with GPS, cycles with an estimate, each as a count out of a total. Click one to see the records. | Ni kiasi gani cha kile kilichorekodiwa kimekaguliwa: watu waliohakikiwa, mashamba yenye GPS, misimu ya zao yenye makadirio, kila kimoja kikiwa idadi kati ya jumla. Bonyeza kimoja uone rekodi. | draft | Flag: Reworded after the cross-check; unreviewed. |
| `tour.ops.towerQualityTitle` | Data quality | Ubora wa taarifa | draft |  |
| `tour.ops.towerTitle` | The Control Tower | Control Tower | draft |  |
| `tour.ops.towerVillageBody` | The Tower is always about one village. Capacity here is planned and always shown with its basis — never measured — and prospective and approved demand are separate figures that are never added together. | Control Tower daima inahusu kijiji kimoja. Uwezo hapa ni uliopangwa, na daima unaonyeshwa pamoja na msingi wake — kamwe si uliopimwa — na mahitaji yanayotarajiwa na yaliyoidhinishwa ni takwimu tofauti ambazo hazijumlishwi kamwe. | draft |  |
| `tour.ops.towerVillageTitle` | Pick a village first | Chagua kijiji kwanza | draft |  |
| `tour.ops.welcomeBody` | This is the programme surface: requests, buyer demand, surveys and the Control Tower. The tour button in the header lets you pick any part of it to be shown at any time. | Hii ni sehemu ya mradi: maombi, mahitaji ya wanunuzi, madodoso na Control Tower. Kitufe cha Ziara kilicho juu kinakuwezesha kuchagua sehemu yoyote ionyeshwe wakati wowote. | draft | Flag: Reworded after the cross-check; unreviewed. |
| `tour.ops.welcomeSidebarBody` | Requests, Demand, Catalogue, Buyers, Villages, Surveys and the Control Tower are each one click away. The tour button in the header replays any part of this tour whenever you want it. | Maombi, Mahitaji, Katalogi, Wanunuzi, Vijiji, Madodoso na Control Tower vyote viko umbali wa kubofya mara moja. Kitufe cha Ziara kilicho juu kinarudia sehemu yoyote ya ziara hii wakati wowote unapotaka. | draft |  |
| `tour.ops.welcomeSidebarTitle` | Everything is in the sidebar | Kila kitu kiko kwenye menyu ya pembeni | draft |  |
| `tour.ops.welcomeTitle` | Welcome to Ruaha 360 | Karibu Ruaha 360 | draft |  |
| `tower.actual` | Actual | Halisi | draft |  |
| `tower.approvedNote` | Decided yes. Still not measured consumption. | Uamuzi ni ndiyo. Bado si matumizi yaliyopimwa. | draft |  |
| `tower.approvedPeak` | Approved peak | Kilele kilichoidhinishwa | draft |  |
| `tower.approvedPerWeek` | Approved, estimated per week | Yaliyoidhinishwa, makadirio kwa wiki | draft |  |
| `tower.available` | Available | Inapatikana | draft |  |
| `tower.backToTower` | Back to the Tower | Rudi kwenye Tower | draft |  |
| `tower.basis` | Basis | Msingi | draft | Capacity is PLANNED or NAMEPLATE, never measured. Always shown with its basis. Flag: Drafter: 'Msingi' for basis has no attested sense here; alternative 'kigezo'. Cross-check (low): Category: natural. 'Msingi' was back-translated as 'Foundation (basis)': its attested sense is 'foundation', so 'basis of a capacity figure' may not be understood; the values it labels ('Uliopangwa' / 'Kibao cha mashine') help. 'Kigezo' is the drafter's alternative but is already used for 'simultaneity factor', so a reviewer should decide. |
| `tower.capacity` | Planned capacity | Uwezo uliopangwa | draft | Capacity is PLANNED or NAMEPLATE, never measured. Always shown with its basis. |
| `tower.chooseVillage` | Choose a village | Chagua kijiji | draft |  |
| `tower.colActual` | Actual | Halisi | draft |  |
| `tower.colActualKg` | Actual kg | Kg halisi | draft | Flag: Reworded after the cross-check; unreviewed. |
| `tower.colApplicant` | Applicant | Mwombaji | draft |  |
| `tower.colApplicantEquipment` | Applicant · equipment | Mwombaji · kifaa | draft |  |
| `tower.colAvailable` | Available | Inapatikana | draft |  |
| `tower.colBuyer` | Buyer | Mnunuzi | draft |  |
| `tower.colBuyerCrop` | Buyer · crop | Mnunuzi · zao | draft |  |
| `tower.colCoverage` | Coverage | Kiwango kilichokidhiwa | draft | Flag: Cross-check (low): Category: length. Three-word column header 'Kiwango kilichokidhiwa' for the single word 'Coverage', with the same 'met = fulfilled' ambiguity as tower.coverage; consider the glossary's short form 'Yaliyokidhiwa (%)'. |
| `tower.colCrop` | Crop | Zao | draft |  |
| `tower.colCropFarmerPlot` | Crop · farmer · plot | Zao · mkulima · kipande | draft | Flag: Reworded after the cross-check; unreviewed. |
| `tower.colCropWindow` | Crop · window | Zao · kipindi | draft |  |
| `tower.colCycles` | Cycles | Misimu ya zao | draft |  |
| `tower.colDemand` | Demand | Mahitaji | draft |  |
| `tower.colEquipment` | Equipment | Kifaa | draft |  |
| `tower.colExpected` | Expected | Inayotarajiwa | draft |  |
| `tower.colExpectedKg` | Expected kg | Kg zinazotarajiwa | draft | Flag: Reworded after the cross-check; unreviewed. |
| `tower.colOpportunity` | Opportunity | Fursa | draft | Flag: Drafter: Just 'Fursa'; the not-a-sale reading depends on surrounding copy. |
| `tower.colPeak` | Est. peak | Kilele (makadirio) | draft |  |
| `tower.colPlanted` | Planted | Lililopandwa | draft |  |
| `tower.colStatus` | Status | Hali | draft |  |
| `tower.colVerified` | Verified cycles | Misimu ya zao iliyohakikiwa | draft |  |
| `tower.colWindow` | Window | Kipindi | draft |  |
| `tower.coverage` | Coverage | Kiwango kilichokidhiwa | draft | Flag: Drafter: 'Kiwango kilichokidhiwa' (level met) for coverage: glossary suggests 'Mahitaji yaliyokidhiwa (%)'; shortened for column use. Cross-check (low): Category: glossary. 'Kiwango kilichokidhiwa' drops 'of demand' and the glossary form 'Mahitaji yaliyokidhiwa (%)'; alone, 'level met' can read as demand fulfilled or delivered, which an opportunity/coverage figure is not. Same wording is used for tower.colCoverage, where it is also much longer than the one-word English header. |
| `tower.cycles` | Cycles | Misimu ya zao | draft |  |
| `tower.cyclesWithEstimate` | Cycles with an estimate | Misimu ya zao yenye makadirio | draft | Always labelled an ESTIMATE. Not a measurement, not a commitment. |
| `tower.drill` | See the records | Tazama rekodi | draft |  |
| `tower.energy` | Energy | Nishati | draft |  |
| `tower.energyDrill` | Requests behind the energy figures | Maombi yaliyo nyuma ya takwimu za nishati | draft |  |
| `tower.excludedFromFigures_one` | 1 more request in this village is draft, rejected or withdrawn. It feeds neither figure — the equipment pipeline tile counts every status. | Ombi 1 zaidi katika kijiji hiki ni rasimu, limekataliwa au limeondolewa. Halichangii takwimu yoyote kati ya hizo mbili — kigae cha mtiririko wa maombi ya vifaa kinahesabu kila hali. | draft |  |
| `tower.excludedFromFigures_other` | {{count}} more requests in this village are draft, rejected or withdrawn. They feed neither figure — the equipment pipeline tile counts every status. | Maombi {{count}} zaidi katika kijiji hiki ni rasimu, yamekataliwa au yameondolewa. Hayachangii takwimu yoyote kati ya hizo mbili — kigae cha mtiririko wa maombi ya vifaa kinahesabu kila hali. | draft | Keep {{count}} |
| `tower.expected` | Expected | Inayotarajiwa | draft |  |
| `tower.expectedWeightFor` | Expected weight, {{crop}} · {{window}} | Uzito unaotarajiwa, {{crop}} · {{window}} | draft | Keep {{crop}} {{window}} |
| `tower.farmsWithGps` | Farms with GPS | Mashamba yenye GPS | draft |  |
| `tower.figureOnTile` | {{crop}}, {{window}} — the figure on the tile | {{crop}}, {{window}} — takwimu iliyo kwenye kigae | draft | Keep {{crop}} {{window}} |
| `tower.headroom` | Headroom | Uwezo uliobaki | draft | Flag: Drafter: 'Uwezo uliobaki' for headroom is an unverified glossary composition. |
| `tower.headroomLeft` | Headroom left | Uwezo uliobaki | draft |  |
| `tower.headroomNote` | Approved peak against planned capacity. Prospective requests are not on this bar; they are not a load. | Kilele kilichoidhinishwa dhidi ya uwezo uliopangwa. Maombi yanayotarajiwa hayamo kwenye upau huu; si mzigo. | draft |  |
| `tower.indicativeValue` | Indicative value | Thamani ya makadirio | draft | Prices are INDICATIVE, never quotations. Must not read as a firm offer. Flag: Drafter: 'Thamani ya makadirio' follows glossary for indicative; catalogue value is not a quotation. |
| `tower.indicativeValueNote` | Indicative catalogue value of requests to buy: price times quantity. Requests to rent show their indicative rent per day instead. Not financed value and not a loan book. | Thamani ya makadirio ya katalogi kwa maombi ya kununua: bei mara kiasi. Maombi ya kukodi yanaonyesha kodi elekezi kwa siku badala yake. Si thamani iliyofadhiliwa wala si daftari la mikopo. | draft | Prices are INDICATIVE, never quotations. Must not read as a firm offer. Flag: Drafter: 'daftari la mikopo' for loan book is unverified. Cross-check (low): Category: meaning. 'iliyofadhiliwa' (from kufadhili, to sponsor or fund as a donor) may read as grant-funded rather than 'financed' in the loan sense. Finance terms are unresolved in the project, so a reviewer should confirm. The indicative-not-a-quotation reading is intact. |
| `tower.lead` | Every headline here drills to the records underneath it. | Kila kichwa cha takwimu hapa kinakupeleka kwenye rekodi zilizo chini yake. | draft | Flag: Drafter: 'Kichwa cha takwimu' for headline is unverified. |
| `tower.market` | Market | Soko | draft |  |
| `tower.marketDrill` | Demand against available supply | Mahitaji dhidi ya ugavi unaopatikana | draft |  |
| `tower.marketNote` | Open demand against supply that is still available. | Mahitaji yaliyo wazi dhidi ya ugavi unaopatikana bado. | draft |  |
| `tower.neverSummed` | Prospective and approved are separate figures and are never added together. | Vilele vinavyotarajiwa na vilivyoidhinishwa ni takwimu tofauti na havijumlishwi kamwe. | draft |  |
| `tower.noCapacityDetail` | A current village_capacity row is needed before energy figures can be shown. | Safu ya village_capacity ya sasa inahitajika kabla takwimu za nishati hazijaonyeshwa. | draft | Capacity is PLANNED or NAMEPLATE, never measured. Always shown with its basis. Flag: Drafter: Left database table name village_capacity as-is, as in English. |
| `tower.noCapacityTitle` | No capacity recorded | Hakuna uwezo uliorekodiwa | draft | Capacity is PLANNED or NAMEPLATE, never measured. Always shown with its basis. |
| `tower.noDataDetail` | This tile fills in as records are created. | Kigae hiki hujazwa rekodi zinapoundwa. | draft |  |
| `tower.noDataTitle` | Nothing recorded yet | Bado hakuna kilichorekodiwa | draft |  |
| `tower.noneInFigure` | No requests in this figure. | Hakuna maombi katika takwimu hii. | draft |  |
| `tower.noVillageDetail` | The Tower reports on one village at a time. | Tower inaripoti kijiji kimoja kwa wakati mmoja. | draft |  |
| `tower.noVillageTitle` | Choose a village | Chagua kijiji | draft |  |
| `tower.openDemand` | Open demand | Mahitaji yaliyo wazi | draft |  |
| `tower.personsVerified` | Persons verified | Watu waliohakikiwa | draft |  |
| `tower.plantedArea` | Planted area across cycles | Jumla ya eneo lililopandwa katika mazao yote | draft | This is planted area ACROSS CYCLES, never "land area": intercropping means it can exceed the village’s hectares. |
| `tower.plantedAreaNote` | The sum of cycle areas. Intercropping means several cycles share a plot, so this can exceed the village’s hectares. It is not land area. | Jumla ya maeneo ya misimu ya zao. Kilimo mseto humaanisha misimu kadhaa ya zao inashiriki kipande kimoja cha shamba, kwa hiyo jumla hii inaweza kuzidi hekta za kijiji. Si eneo la ardhi. | draft | This is planted area ACROSS CYCLES, never "land area": intercropping means it can exceed the village’s hectares. Flag: Drafter: Uses glossary planted-area-across-cycles decision; 'kilimo mseto' for intercropping is unverified; 'hekta za kijiji' assumes village hectares. |
| `tower.production` | Production | Uzalishaji | draft |  |
| `tower.productionDrill` | Production by crop and window | Uzalishaji kwa zao na kipindi | draft |  |
| `tower.productionNote` | Expected and actual harvest by crop and window. | Mavuno yanayotarajiwa na halisi kwa zao na kipindi. | draft |  |
| `tower.prospectiveNote` | Requests submitted or under review. An application, not a load. | Maombi yaliyowasilishwa au yanayopitiwa. Ni maombi, si mzigo. | draft | Prospective and approved demand are separate figures and are NEVER summed. |
| `tower.prospectivePeak` | Prospective peak | Kilele kinachotarajiwa | draft | Prospective and approved demand are separate figures and are NEVER summed. Flag: Drafter: 'Kilele kinachotarajiwa'/'kilichoidhinishwa' extend existing 'Kilele kilichokadiriwa'; unverified. Cross-check (low): Category: meaning. 'Kilele kinachotarajiwa' (expected peak) suggests a forecast that will happen, whereas English 'prospective' means peaks from submitted or under-review applications that may never be approved. The tower.prospectiveNote ('Ni maombi, si mzigo') mitigates this. The same root is used for 'Expected' harvest, so 'prospective' and 'expected' collapse into one Swahili word. The keep-separate-from-approved rule itself is preserved. |
| `tower.pue` | Equipment pipeline | Mtiririko wa maombi ya vifaa | draft | Flag: Drafter: 'Mtiririko wa maombi ya vifaa' for 'Equipment pipeline' is a composition; reviewer to confirm. |
| `tower.pueNote` | Requests by status, with the catalogue value they represent. | Maombi kwa hali yake, pamoja na thamani ya katalogi inayowakilisha. | draft |  |
| `tower.quality` | Data quality | Ubora wa taarifa | draft | Flag: Drafter: 'Ubora wa taarifa' for data quality is unverified. |
| `tower.qualityCounted` | Counted | Imehesabiwa | draft |  |
| `tower.qualityDrill` | Records behind data quality | Rekodi zilizo nyuma ya ubora wa taarifa | draft |  |
| `tower.qualityMetrics` | Data quality measures | Vipimo vya ubora wa taarifa | draft |  |
| `tower.qualityMissing` | Not counted | Haijahesabiwa | draft |  |
| `tower.qualityNote` | How much of what has been recorded has been checked. | Kiasi cha taarifa zilizorekodiwa ambazo tayari zimekaguliwa. | draft |  |
| `tower.qualityRecord` | Record | Rekodi | draft |  |
| `tower.qualityState` | In measure | Katika kipimo | draft | Flag: Drafter: 'Katika kipimo' (in measure) is a literal rendering of 'In measure'; ambiguous, needs check against screen. Cross-check (medium): Category: meaning. 'Katika kipimo' (back: 'In the measure') is opaque, and 'kipimo' elsewhere means 'measurement' (e.g. 'si kipimo'), so it can read as 'measured', which sits badly with the never-measured rule; the English means 'included in this quality metric'. |
| `tower.rentPerDay` | + {{value}} rent per day | + {{value}} kodi kwa siku | draft | Keep {{value}} Flag: Drafter: 'kodi' can also mean tax; reviewer to confirm it reads as rent here. |
| `tower.requestCount_one` | 1 request | Ombi 1 | draft |  |
| `tower.requestCount_other` | {{count}} requests | Maombi {{count}} | draft | Keep {{count}} |
| `tower.requests` | Requests | Maombi | draft |  |
| `tower.simultaneity` | Simultaneity factor | Kigezo cha matumizi ya wakati mmoja | draft | Flag: Drafter: 'Kigezo cha matumizi ya wakati mmoja' is unverified (no Tanzanian source); energy-sector reviewer needed. |
| `tower.simultaneityNote` | Applied to the peaks above. Village peak is not the sum of rated power. | Kinatumika kwenye vilele vilivyo hapo juu. Kilele cha kijiji si jumla ya nguvu iliyoandikwa. | draft |  |
| `tower.stillFree` | Still free | Bado unapatikana | draft | Flag: Drafter: 'Bado uko huru' is awkward; agreement with kilele/uwezo unclear, reviewer to improve. Reworded after the cross-check; unreviewed. |
| `tower.sumOfPeaks` | Sum of estimated peaks | Jumla ya vilele vilivyokadiriwa | draft |  |
| `tower.timesFactor` | × {{factor}} simultaneity = | × kigezo cha matumizi ya wakati mmoja {{factor}} = | draft | Keep {{factor}} |
| `tower.title` | Control Tower | Control Tower | draft |  |
| `tower.trees` | Trees | Miti | draft |  |
| `tower.village` | Village | Kijiji | draft |  |
| `villages.basis` | Capacity basis | Msingi wa uwezo | draft | Capacity is PLANNED or NAMEPLATE, never measured. Always shown with its basis. |
| `villages.capacity` | Planned capacity (kW) | Uwezo uliopangwa (kW) | draft | Capacity is PLANNED or NAMEPLATE, never measured. Always shown with its basis. |
| `villages.code` | Code | Msimbo | draft |  |
| `villages.colBasis` | Basis | Msingi | draft |  |
| `villages.colCapacity` | Planned capacity | Uwezo uliopangwa | draft | Capacity is PLANNED or NAMEPLATE, never measured. Always shown with its basis. |
| `villages.colCode` | Code | Msimbo | draft |  |
| `villages.colEffectiveFrom` | Effective from | Inatumika kuanzia | draft | Flag: Drafter: 'Inatumika kuanzia' for Effective from; composed. |
| `villages.colName` | Village | Kijiji | draft |  |
| `villages.colSimultaneity` | Simultaneity factor | Kigezo cha matumizi ya wakati mmoja | draft | Flag: Cross-check (low): Same long, unverified term as ops.simultaneity, used as a table column header. |
| `villages.create` | Add village | Ongeza kijiji | draft |  |
| `villages.createTitle` | New village | Kijiji kipya | draft |  |
| `villages.creating` | Adding… | Inaongeza… | draft |  |
| `villages.effectiveFrom` | Effective from | Inatumika kuanzia | draft |  |
| `villages.gpsHint` | Optional. Decimal degrees, for example -7.91. | Si lazima. Nyuzi kwa desimali, kwa mfano -7.91. | draft |  |
| `villages.latitude` | Latitude | Latitudo | draft |  |
| `villages.longitude` | Longitude | Longitudo | draft |  |
| `villages.name` | Village name | Jina la kijiji | draft |  |
| `villages.noneDetail` | No villages are visible for this project. | Hakuna vijiji vinavyoonekana kwa mradi huu. | draft |  |
| `villages.noneTitle` | No villages | Hakuna vijiji | draft |  |
| `villages.notANumber` | Enter a number, using a point for decimals. | Andika namba, ukitumia nukta kwa desimali. | draft |  |
| `villages.officerNote` | Field officers see a new village only once they are assigned to it. | Maafisa wa uwandani wataona kijiji kipya pale tu watakapopangiwa. | draft |  |
| `villages.plannedNote` | Capacity is planned, never measured. Every figure is shown with the basis it was planned on. | Uwezo umepangwa, haujawahi kupimwa. Kila takwimu inaonyeshwa pamoja na msingi uliotumika kuipanga. | draft |  |
| `villages.required` | Required. | Hii ni lazima. | draft |  |
| `villages.simultaneity` | Simultaneity factor | Kigezo cha matumizi ya wakati mmoja | draft |  |
| `villages.simultaneityHint` | Between 0 and 1. Leave 1 if not yet planned. | Kati ya 0 na 1. Acha 1 kama bado haijapangwa. | draft |  |
| `villages.simultaneityNote` | The simultaneity factor is applied to village peaks. Village peak is not the sum of rated power. | Kigezo cha matumizi ya wakati mmoja hutumika kwenye kilele cha vijiji. Kilele cha kijiji si jumla ya nguvu zilizoandikwa. | draft |  |
| `villages.sourceNote` | Where the figure comes from | Takwimu inatoka wapi | draft | Provenance wording: where a record came from, and who verified it. |
| `villages.title` | Villages | Vijiji | draft |  |

