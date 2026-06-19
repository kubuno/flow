import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import * as Icons from 'lucide-react'
import { X, Search, ChevronRight, ChevronDown } from 'lucide-react'
import type { NodeMeta } from './types'

function LucideIcon({ name, size = 16, color }: { name: string; size?: number; color?: string }) {
  const Cmp = (Icons as unknown as Record<string, React.ComponentType<{ size?: number; color?: string }>>)[name] ?? Icons.Box
  return <Cmp size={size} color={color} />
}

/** Regroupement « intelligent » : par usage plutôt que par catégorie technique brute. */
interface Group { id: string; label: string; match: (m: NodeMeta) => boolean }

export default function NodePicker({ catalog, onPick, onClose }: { catalog: NodeMeta[]; onPick: (meta: NodeMeta) => void; onClose: () => void }) {
  const { t } = useTranslation('flow')
  const [q, setQ] = useState('')
  const [open, setOpen] = useState<Set<string>>(new Set()) // sections dépliées (repliées par défaut)

  const groups: Group[] = useMemo(() => [
    { id: 'trigger', label: t('grp_triggers', { defaultValue: 'Déclencheurs' }),        match: m => m.category === 'trigger' },
    { id: 'logic',   label: t('grp_logic',    { defaultValue: 'Logique & données' }),   match: m => m.category === 'logic' && !m.type.startsWith('flow.') },
    { id: 'db',      label: t('grp_db',       { defaultValue: 'Bases de données' }),     match: m => m.type.startsWith('db.') },
    { id: 'ai',      label: t('grp_ai',       { defaultValue: 'IA & Agents' }),          match: m => m.category === 'ai' || m.type === 'external.ai' },
    { id: 'http',    label: t('grp_http',     { defaultValue: 'HTTP & API' }),           match: m => m.type === 'external.http_request' },
    { id: 'flow',    label: t('grp_flow',     { defaultValue: 'Sous-workflows' }),       match: m => m.type.startsWith('flow.') },
    { id: 'kubuno',  label: t('grp_kubuno',   { defaultValue: 'Applications Kubuno' }),  match: m => m.category === 'kubuno' },
    { id: 'code',    label: t('grp_code',     { defaultValue: 'Code' }),                 match: m => m.category === 'code' },
    { id: 'integration', label: t('grp_integrations', { defaultValue: 'Intégrations' }), match: m => m.category === 'integration' },
  ], [t])

  const searchMode = q.trim() !== ''

  // Assigne chaque nœud (filtré) à son premier groupe correspondant.
  const buckets = useMemo(() => {
    const ql = q.toLowerCase()
    const filtered = catalog.filter(m => !ql || m.name.toLowerCase().includes(ql) || m.type.toLowerCase().includes(ql) || (m.description ?? '').toLowerCase().includes(ql))
    const map = new Map<string, NodeMeta[]>()
    for (const m of filtered) {
      const g = groups.find(g => g.match(m))?.id ?? 'logic'
      const arr = map.get(g) ?? []; arr.push(m); map.set(g, arr)
    }
    for (const arr of map.values()) arr.sort((a, b) => a.name.localeCompare(b.name))
    return map
  }, [catalog, q, groups])

  const toggle = (id: string) => setOpen(s => { const n = new Set(s); if (n.has(id)) n.delete(id); else n.add(id); return n })

  return (
    <div className="absolute inset-0 z-30 bg-black/40 flex justify-end" onClick={onClose}>
      <div className="w-80 h-full bg-[#ffffff] border-l border-[#dadce0] flex flex-col" onClick={e => e.stopPropagation()}>
        <div className="flex items-center justify-between px-3 py-2 border-b border-[#dadce0]">
          <span className="text-[#5f6368] text-sm font-semibold">{t('add_node_title')}</span>
          <button className="text-[#80868b] hover:text-[#202124]" onClick={onClose}><X size={18} /></button>
        </div>
        <div className="p-2 border-b border-[#dadce0]">
          <div className="flex items-center gap-2 bg-[#e8eaed] rounded px-2 py-1.5">
            <Search size={14} className="text-[#80868b]" />
            <input autoFocus value={q} onChange={e => setQ(e.target.value)} placeholder={t('search_placeholder')}
              className="bg-transparent text-sm text-[#202124] outline-none w-full" />
          </div>
        </div>
        <div className="flex-1 overflow-y-auto p-1.5">
          {groups.filter(g => buckets.has(g.id)).map(g => {
            const items = buckets.get(g.id)!
            const expanded = searchMode || open.has(g.id)
            return (
              <div key={g.id} className="mb-0.5">
                <button onClick={() => toggle(g.id)}
                  className="w-full flex items-center gap-1.5 px-2 py-1.5 rounded hover:bg-[#f1f3f4] text-left">
                  {expanded ? <ChevronDown size={14} className="text-[#80868b]" /> : <ChevronRight size={14} className="text-[#80868b]" />}
                  <span className="text-xs font-semibold text-[#5f6368] flex-1">{g.label}</span>
                  <span className="text-[10px] text-[#9aa0a6] bg-[#f1f3f4] rounded-full px-1.5">{items.length}</span>
                </button>
                {expanded && (
                  <div className="pl-1 space-y-0.5 pt-0.5">
                    {items.map(m => (
                      <button key={m.type} onClick={() => onPick(m)}
                        className="w-full flex items-center gap-2 px-2 py-1.5 rounded hover:bg-[#e8eaed] text-left">
                        <span className="w-7 h-7 rounded flex items-center justify-center flex-shrink-0" style={{ background: m.color }}>
                          <LucideIcon name={m.icon} size={15} color="#fff" />
                        </span>
                        <span className="min-w-0">
                          <span className="block text-sm text-[#202124] truncate">{m.name}</span>
                          <span className="block text-[11px] text-[#80868b] truncate">{m.description}</span>
                        </span>
                      </button>
                    ))}
                  </div>
                )}
              </div>
            )
          })}
          {buckets.size === 0 && <div className="text-center text-[#80868b] text-sm py-8">{t('no_nodes')}</div>}
        </div>
      </div>
    </div>
  )
}
