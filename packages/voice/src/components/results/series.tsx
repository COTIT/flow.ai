/**
 * A series as a picture: a trend line or bars. Drawing scale only (aria-hidden): every number is
 * shown elsewhere, from the API. Shared by voice results and Flow's scene charts.
 */
export function Series({ values, line, className = "h-28" }: { values: number[]; line: boolean; className?: string }) {
  const top = Math.max(...values, 1)
  const step = 100 / Math.max(values.length - 1, 1)
  if (line)
    return (
      <svg aria-hidden viewBox="0 0 100 40" preserveAspectRatio="none" className={`${className} w-full text-(--gold)`}>
        <polyline
          fill="none"
          stroke="currentColor"
          strokeWidth="1.5"
          vectorEffect="non-scaling-stroke"
          points={values.map((v, i) => `${i * step},${40 - (v / top) * 38}`).join(" ")}
        />
      </svg>
    )
  return (
    <div aria-hidden className={`flex ${className} items-end gap-1`}>
      {values.map((v, i) => (
        <div key={i} className="flex-1 rounded-t bg-(--gold)/70" style={{ height: `${Math.max((v / top) * 100, 2)}%` }} />
      ))}
    </div>
  )
}
