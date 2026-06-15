import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import * as Icons from 'lucide-react'
import { X, Search } from 'lucide-react'
import type { NodeCategory, NodeMeta } from './types'

const CATEGORY_KEYS: Record<NodeCategory, string> = {
  trigger:  'cat_triggers',
  kubuno:   'cat_kubuno',
  logic:    'cat_logic',
  external: 'cat_external',
  code:     'cat_code',
}
const ORDER: NodeCategory[] = ['trigger', 'kubuno', 'logic', 'external', 'code']

function LucideIcon({ name, size = 16, color }: { name: string; size?: number; color?: string }) {
  const Cmp = (Icons as unknown as Record<string, React.ComponentType<{ size?: number; color?: string }>>)[name] ?? Icons.Box
  return <Cmp size={size} color={color} />
}

export default function NodePicker({ catalog, onPick, onClose }: { catalog: NodeMeta[]; onPick: (meta: NodeMeta) => void; onClose: () => void }) {
  const { t } = useTranslation('flow')
  const [q, setQ] = useState('')
  const groups = useMemo(() => {
    const filtered = catalog.filter(m =>
      !q || m.name.toLowerCase().includes(q.toLowerCase()) || m.type.includes(q.toLowerCase()))
    const map = new Map<NodeCategory, NodeMeta[]>()
    for (const m of filtered) {
      const arr = map.get(m.category) ?? []
      arr.push(m); map.set(m.category, arr)
    }
    return map
  }, [catalog, q])

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
        <div className="flex-1 overflow-y-auto p-2 space-y-3">
          {ORDER.filter(c => groups.has(c)).map(cat => (
            <div key={cat}>
              <div className="text-[10px] uppercase tracking-wider text-[#80868b] px-1 mb-1">{t(CATEGORY_KEYS[cat])}</div>
              <div className="space-y-1">
                {groups.get(cat)!.map(m => (
                  <button key={m.type} onClick={() => onPick(m)}
                    className="w-full flex items-center gap-2 px-2 py-2 rounded hover:bg-[#e8eaed] text-left">
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
            </div>
          ))}
          {groups.size === 0 && <div className="text-center text-[#80868b] text-sm py-8">{t('no_nodes')}</div>}
        </div>
      </div>
    </div>
  )
}
