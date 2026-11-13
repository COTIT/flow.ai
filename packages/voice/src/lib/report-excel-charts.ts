import type { Kind } from "./views"

export type WorkbookChart = {
  title: string
  kind: Kind
  categories: string[]
  values: (number | null)[]
  categoryRef: string
  valueRef: string
  numberFormat: string
  col: number
  row: number
  width: number
  height: number
}

const CHART = "http://schemas.openxmlformats.org/drawingml/2006/chart"
const DRAWING = "http://schemas.openxmlformats.org/drawingml/2006/main"
const REL =
  "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
const XML = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
const xml = (value: string | number) =>
  String(value)
    // eslint-disable-next-line no-control-regex -- XML 1.0 rejects these characters in ERP labels.
    .replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\ufffe\uffff]/g, "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&apos;")
const font =
  '<a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr sz="950"><a:solidFill><a:srgbClr val="252622"/></a:solidFill><a:latin typeface="Poppins"/></a:defRPr></a:pPr><a:endParaRPr lang="en-US"/></a:p>'
const fill = (color: string) =>
  `<a:solidFill><a:srgbClr val="${color}"/></a:solidFill>`

function chartXml(chart: WorkbookChart) {
  const polar = chart.kind === "pie" || chart.kind === "donut"
  const line = chart.kind === "line" || chart.kind === "sparkline"
  const horizontal = chart.kind === "hbar"
  const largest = Math.max(
    0,
    ...chart.values.map((value) => Math.abs(value ?? 0))
  )
  const roughStep = largest / 4 || 1
  const power = 10 ** Math.floor(Math.log10(roughStep))
  const multiple = roughStep / power
  const majorUnit =
    (multiple <= 1 ? 1 : multiple <= 2 ? 2 : multiple <= 5 ? 5 : 10) * power
  const axisFormat =
    largest >= 1e9 || (largest > 0 && largest < 0.01)
      ? "0.##E+0"
      : `#,##0${majorUnit < 1 ? `.${"#".repeat(Math.min(8, 1 - Math.floor(Math.log10(majorUnit))))}` : ""}`
  const colors = ["C5A15B", "7C6844", "D6BE90", "7B8170", "B5BBA9", "444D40"]
  const points = chart.categories
    .map((category, i) => `<c:pt idx="${i}"><c:v>${xml(category)}</c:v></c:pt>`)
    .join("")
  const values = chart.values
    .flatMap((value, i) =>
      value === null ? [] : [`<c:pt idx="${i}"><c:v>${value}</c:v></c:pt>`]
    )
    .join("")
  const series = `<c:ser><c:idx val="0"/><c:order val="0"/><c:tx><c:v>${xml(chart.title)}</c:v></c:tx><c:spPr>${line ? `<a:ln w="28575">${fill("C5A15B")}</a:ln>` : fill("C5A15B")}</c:spPr>${line ? '<c:marker><c:symbol val="none"/></c:marker>' : ""}${polar ? chart.values.map((_, i) => `<c:dPt><c:idx val="${i}"/><c:spPr>${fill(colors[i % colors.length]!)}</c:spPr></c:dPt>`).join("") : ""}<c:cat><c:strRef><c:f>${xml(chart.categoryRef)}</c:f><c:strCache><c:ptCount val="${chart.categories.length}"/>${points}</c:strCache></c:strRef></c:cat><c:val><c:numRef><c:f>${xml(chart.valueRef)}</c:f><c:numCache><c:formatCode>${xml(chart.numberFormat)}</c:formatCode><c:ptCount val="${chart.values.length}"/>${values}</c:numCache></c:numRef></c:val>${line ? '<c:smooth val="0"/>' : ""}</c:ser>`
  const axes = polar
    ? ""
    : `<c:catAx><c:axId val="1"/><c:scaling><c:orientation val="${horizontal ? "maxMin" : "minMax"}"/></c:scaling><c:axPos val="${horizontal ? "l" : "b"}"/><c:majorTickMark val="none"/><c:minorTickMark val="none"/><c:tickLblPos val="nextTo"/><c:txPr>${font}</c:txPr><c:crossAx val="2"/><c:crosses val="autoZero"/><c:auto val="1"/><c:lblAlgn val="ctr"/><c:lblOffset val="100"/><c:tickLblSkip val="${Math.max(1, Math.ceil(chart.categories.length / (horizontal ? 15 : 6)))}"/></c:catAx><c:valAx><c:axId val="2"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:axPos val="${horizontal ? "b" : "l"}"/><c:majorGridlines><c:spPr><a:ln>${fill("E5E2DA")}</a:ln></c:spPr></c:majorGridlines><c:numFmt formatCode="${xml(axisFormat)}" sourceLinked="0"/><c:majorTickMark val="none"/><c:minorTickMark val="none"/><c:tickLblPos val="nextTo"/><c:txPr>${font}</c:txPr><c:crossAx val="1"/><c:crosses val="${horizontal ? "max" : "autoZero"}"/><c:crossBetween val="between"/><c:majorUnit val="${majorUnit}"/></c:valAx>`
  const plot = polar
    ? chart.kind === "donut"
      ? `<c:doughnutChart><c:varyColors val="1"/>${series}<c:firstSliceAng val="270"/><c:holeSize val="55"/></c:doughnutChart>`
      : `<c:pieChart><c:varyColors val="1"/>${series}<c:firstSliceAng val="270"/></c:pieChart>`
    : line
      ? `<c:lineChart><c:grouping val="standard"/><c:varyColors val="0"/>${series}<c:axId val="1"/><c:axId val="2"/></c:lineChart>`
      : `<c:barChart><c:barDir val="${horizontal ? "bar" : "col"}"/><c:grouping val="clustered"/><c:varyColors val="0"/>${series}<c:gapWidth val="80"/><c:axId val="1"/><c:axId val="2"/></c:barChart>`
  return `${XML}<c:chartSpace xmlns:c="${CHART}" xmlns:a="${DRAWING}"><c:lang val="en-US"/><c:chart><c:title><c:tx><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr sz="1250" b="1"><a:latin typeface="Poppins"/></a:rPr><a:t>${xml(chart.title)}</a:t></a:r></a:p></c:rich></c:tx><c:overlay val="0"/></c:title><c:plotArea><c:layout/>${plot}${axes}</c:plotArea>${polar ? `<c:legend><c:legendPos val="r"/><c:overlay val="0"/><c:txPr>${font}</c:txPr></c:legend>` : ""}<c:plotVisOnly val="0"/><c:dispBlanksAs val="gap"/></c:chart><c:spPr>${fill("FFFFFF")}<a:ln><a:noFill/></a:ln></c:spPr><c:txPr>${font}</c:txPr></c:chartSpace>`
}

/** ExcelJS has no chart writer. Add standard native charts, linked to the workbook's editable data. */
export async function addWorkbookCharts(
  bytes: Uint8Array,
  charts: WorkbookChart[]
): Promise<Uint8Array> {
  const { default: JSZip } = await import("jszip")
  const zip = await JSZip.loadAsync(bytes)
  // ExcelJS marks hidden totals as shown; make the absence explicit for other spreadsheet readers.
  for (const path of Object.keys(zip.files).filter((path) =>
    /^xl\/tables\/table\d+\.xml$/.test(path)
  )) {
    const table = await zip.file(path)!.async("string")
    zip.file(path, table.replace('totalsRowShown="1"', 'totalsRowShown="0"'))
  }
  if (!charts.length)
    return zip.generateAsync({ type: "uint8array", compression: "DEFLATE" })
  const rels = charts
    .map(
      (_, i) =>
        `<Relationship Id="rId${i + 1}" Type="${REL}/chart" Target="../charts/chart${i + 1}.xml"/>`
    )
    .join("")
  const anchors = charts
    .map((chart, i) => {
      zip.file(`xl/charts/chart${i + 1}.xml`, chartXml(chart))
      return `<xdr:oneCellAnchor><xdr:from><xdr:col>${chart.col}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>${chart.row}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:ext cx="${Math.round(chart.width * 9525)}" cy="${Math.round(chart.height * 9525)}"/><xdr:graphicFrame macro=""><xdr:nvGraphicFramePr><xdr:cNvPr id="${i + 2}" name="Chart ${i + 1}"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/></xdr:xfrm><a:graphic><a:graphicData uri="${CHART}"><c:chart xmlns:c="${CHART}" xmlns:r="${REL}" r:id="rId${i + 1}"/></a:graphicData></a:graphic></xdr:graphicFrame><xdr:clientData/></xdr:oneCellAnchor>`
    })
    .join("")
  zip.file(
    "xl/drawings/drawing1.xml",
    `${XML}<xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="${DRAWING}">${anchors}</xdr:wsDr>`
  )
  zip.file(
    "xl/drawings/_rels/drawing1.xml.rels",
    `${XML}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">${rels}</Relationships>`
  )
  const sheetPath = "xl/worksheets/sheet1.xml"
  const sheet = await zip.file(sheetPath)!.async("string")
  zip.file(
    sheetPath,
    sheet.replace(
      /(<tableParts\b|<extLst\b|<\/worksheet>)/,
      '<drawing r:id="rIdNativeCharts"/>$1'
    )
  )
  const sheetRels = "xl/worksheets/_rels/sheet1.xml.rels"
  const existing =
    (await zip.file(sheetRels)?.async("string")) ??
    `${XML}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"></Relationships>`
  zip.file(
    sheetRels,
    existing.replace(
      "</Relationships>",
      `<Relationship Id="rIdNativeCharts" Type="${REL}/drawing" Target="../drawings/drawing1.xml"/></Relationships>`
    )
  )
  const types = await zip.file("[Content_Types].xml")!.async("string")
  zip.file(
    "[Content_Types].xml",
    types.replace(
      "</Types>",
      `<Override PartName="/xl/drawings/drawing1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawing+xml"/>${charts.map((_, i) => `<Override PartName="/xl/charts/chart${i + 1}.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.chart+xml"/>`).join("")}</Types>`
    )
  )
  return zip.generateAsync({ type: "uint8array", compression: "DEFLATE" })
}
