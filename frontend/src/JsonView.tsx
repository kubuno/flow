import { useState } from 'react'
import { ChevronRight, ChevronDown } from 'lucide-react'

/** Collapsible JSON tree — read-only viewer for node input/output data. */
export default function JsonView({ data, depth = 0, label }: { data: unknown; depth?: number; label?: string }) {
  const [open, setOpen] = useState(depth < 2)

  const isArr = Array.isArray(data)
  const isObj = data !== null && typeof data === 'object' && !isArr

  if (!isArr && !isObj) {
    return (
      <div className="flex gap-1.5 leading-relaxed" style={{ paddingLeft: depth * 12 }}>
        {label !== undefined && <span className="text-[#8a5cf6]">{label}:</span>}
        <span className={valueClass(data)}>{renderScalar(data)}</span>
      </div>
    )
  }

  const entries: [string, unknown][] = isArr
    ? (data as unknown[]).map((v, i) => [String(i), v])
    : Object.entries(data as Record<string, unknown>)
  const brackets = isArr ? ['[', ']'] : ['{', '}']
  const summary = isArr ? `${entries.length}` : `${entries.length}`

  return (
    <div style={{ paddingLeft: depth * 12 }}>
      <button className="flex items-center gap-1 text-left hover:bg-[#e8eaed] rounded px-0.5 -ml-0.5" onClick={() => setOpen(o => !o)}>
        {open ? <ChevronDown size={12} className="text-[#80868b]" /> : <ChevronRight size={12} className="text-[#80868b]" />}
        {label !== undefined && <span className="text-[#8a5cf6]">{label}:</span>}
        <span className="text-[#80868b]">{brackets[0]}{!open && <span className="opacity-60"> {summary} </span>}{!open && brackets[1]}</span>
      </button>
      {open && (
        <div>
          {entries.map(([k, v]) => (
            <JsonView key={k} data={v} depth={depth + 1} label={isArr ? undefined : k} />
          ))}
          <div className="text-[#80868b]" style={{ paddingLeft: (depth + 1) * 12 }}>{brackets[1]}</div>
        </div>
      )}
    </div>
  )
}

function renderScalar(v: unknown): string {
  if (v === null) return 'null'
  if (typeof v === 'string') return `"${v}"`
  return String(v)
}
function valueClass(v: unknown): string {
  if (v === null) return 'text-[#80868b]'
  if (typeof v === 'string') return 'text-[#1e8e3e]'
  if (typeof v === 'number') return 'text-[#1a73e8]'
  if (typeof v === 'boolean') return 'text-[#d93025]'
  return 'text-[#202124]'
}
