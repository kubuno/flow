import { useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { FunctionSquare, Variable, X } from 'lucide-react'
import type { ExprHelp } from './types'

/**
 * Expression input with a `{{ }}` helper. The "fx" button opens a panel listing
 * the available context variables and functions; clicking one inserts a snippet
 * at the caret. Mirrors the expression authoring experience of n8n / Make.
 */
export default function ExpressionField({
  value, onChange, placeholder, help,
}: { value: string; onChange: (v: string) => void; placeholder?: string; help?: ExprHelp }) {
  const { t } = useTranslation('flow')
  const ref = useRef<HTMLInputElement>(null)
  const [open, setOpen] = useState(false)
  const [q, setQ] = useState('')

  const base = 'w-full bg-[#e8eaed] border border-[#dadce0] rounded px-2 py-1.5 text-sm text-[#202124] outline-none focus:border-[#e8824a] font-mono'

  const insert = (snippet: string) => {
    const el = ref.current
    const cur = value ?? ''
    const start = el?.selectionStart ?? cur.length
    const end = el?.selectionEnd ?? cur.length
    const next = cur.slice(0, start) + snippet + cur.slice(end)
    onChange(next)
    setOpen(false)
    requestAnimationFrame(() => {
      el?.focus()
      const pos = start + snippet.length
      el?.setSelectionRange(pos, pos)
    })
  }

  const ql = q.toLowerCase()
  const vars = (help?.variables ?? []).filter(v => !ql || v.name.toLowerCase().includes(ql) || v.description.toLowerCase().includes(ql))
  const fns = (help?.functions ?? []).filter(f => !ql || f.name.toLowerCase().includes(ql) || (f.signature ?? '').toLowerCase().includes(ql))

  return (
    <div className="relative">
      <div className="flex items-stretch gap-1">
        <div className="relative flex-1">
          <input ref={ref} className={base + ' pr-2'} value={value ?? ''} onChange={e => onChange(e.target.value)} placeholder={placeholder} />
        </div>
        <button type="button" onClick={() => setOpen(o => !o)}
          title={t('expr_helper', { defaultValue: 'Insérer une variable / fonction' })}
          className={`shrink-0 px-2 rounded border text-xs font-semibold ${open ? 'bg-[#e8824a] text-white border-[#e8824a]' : 'bg-[#e8eaed] text-[#5f6368] border-[#dadce0] hover:bg-[#dadce0]'}`}>
          fx
        </button>
      </div>

      {open && (
        <div className="absolute right-0 z-40 mt-1 w-72 max-h-80 overflow-y-auto bg-white border border-[#dadce0] rounded-lg shadow-xl">
          <div className="flex items-center justify-between px-2 py-1.5 border-b border-[#dadce0] sticky top-0 bg-white">
            <input autoFocus value={q} onChange={e => setQ(e.target.value)} placeholder={t('search_placeholder', { defaultValue: 'Rechercher…' })}
              className="text-xs outline-none flex-1 bg-transparent" />
            <button onClick={() => setOpen(false)} className="text-[#80868b] hover:text-[#202124]"><X size={14} /></button>
          </div>
          {vars.length > 0 && (
            <div className="py-1">
              <div className="px-2 py-0.5 text-[10px] uppercase tracking-wider text-[#80868b] flex items-center gap-1"><Variable size={11} /> {t('expr_variables', { defaultValue: 'Variables' })}</div>
              {vars.map(v => (
                <button key={v.name} onClick={() => insert(`{{ ${v.name} }}`)} className="w-full text-left px-2 py-1 hover:bg-[#e8eaed]">
                  <div className="text-xs font-mono text-[#8a5cf6]">{v.name}</div>
                  <div className="text-[10px] text-[#80868b]">{v.description}</div>
                </button>
              ))}
            </div>
          )}
          {fns.length > 0 && (
            <div className="py-1 border-t border-[#dadce0]">
              <div className="px-2 py-0.5 text-[10px] uppercase tracking-wider text-[#80868b] flex items-center gap-1"><FunctionSquare size={11} /> {t('expr_functions', { defaultValue: 'Fonctions' })}</div>
              {fns.map(f => (
                <button key={f.name} onClick={() => insert(`{{ ${f.name}() }}`)} className="w-full text-left px-2 py-1 hover:bg-[#e8eaed]">
                  <div className="text-xs font-mono text-[#1a73e8]">{f.signature ?? f.name}</div>
                  <div className="text-[10px] text-[#80868b]">{f.description}</div>
                </button>
              ))}
            </div>
          )}
          {vars.length === 0 && fns.length === 0 && (
            <div className="px-2 py-4 text-center text-xs text-[#80868b]">{t('no_results', { defaultValue: 'Aucun résultat' })}</div>
          )}
        </div>
      )}
    </div>
  )
}
