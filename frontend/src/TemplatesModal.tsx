import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import * as Icons from 'lucide-react'
import { X, FilePlus2 } from 'lucide-react'
import { TEMPLATES, type WorkflowTemplate } from './templates'

function LucideIcon({ name, size = 20 }: { name: string; size?: number }) {
  const Cmp = (Icons as unknown as Record<string, React.ComponentType<{ size?: number }>>)[name] ?? Icons.Workflow
  return <Cmp size={size} />
}

/** Gallery of ready-to-use workflow templates. */
export default function TemplatesModal({ onPick, onClose, busy }: {
  onPick: (tpl: WorkflowTemplate) => void
  onClose: () => void
  busy?: boolean
}) {
  const { t } = useTranslation('flow')
  const [sel, setSel] = useState<string | null>(null)

  return (
    <div className="fixed inset-0 z-50 bg-black/40 flex items-center justify-center p-6" onClick={onClose}>
      <div className="bg-white rounded-xl shadow-2xl w-full max-w-2xl max-h-[80vh] flex flex-col" onClick={e => e.stopPropagation()}>
        <div className="flex items-center justify-between px-4 py-3 border-b border-[#dadce0]">
          <h2 className="text-sm font-semibold text-[#202124]">{t('templates_title', { defaultValue: 'Partir d\'un modèle' })}</h2>
          <button onClick={onClose} className="text-[#80868b] hover:text-[#202124]"><X size={18} /></button>
        </div>
        <div className="flex-1 overflow-y-auto p-4 grid grid-cols-2 gap-3">
          {/* Blank workflow */}
          <button onClick={() => onPick({ id: '__blank__', name: t('new_workflow'), description: '', icon: 'FilePlus2', definition: { nodes: [], edges: [] } })}
            disabled={busy}
            className="text-left border-2 border-dashed border-[#dadce0] rounded-lg p-3 hover:border-[#e8824a] hover:bg-[#fff7f2] transition disabled:opacity-50">
            <div className="w-9 h-9 rounded-lg bg-[#e8eaed] flex items-center justify-center text-[#5f6368] mb-2"><FilePlus2 size={18} /></div>
            <div className="text-sm font-medium text-[#202124]">{t('blank_workflow', { defaultValue: 'Workflow vierge' })}</div>
            <div className="text-[11px] text-[#80868b]">{t('blank_workflow_desc', { defaultValue: 'Partir de zéro.' })}</div>
          </button>

          {TEMPLATES.map(tpl => (
            <button key={tpl.id} onClick={() => { setSel(tpl.id); onPick(tpl) }} disabled={busy}
              className={`text-left border rounded-lg p-3 transition hover:border-[#e8824a] hover:bg-[#fff7f2] disabled:opacity-50 ${sel === tpl.id ? 'border-[#e8824a] bg-[#fff7f2]' : 'border-[#dadce0]'}`}>
              <div className="w-9 h-9 rounded-lg bg-[#e8824a] text-white flex items-center justify-center mb-2"><LucideIcon name={tpl.icon} size={18} /></div>
              <div className="text-sm font-medium text-[#202124]">{tpl.name}</div>
              <div className="text-[11px] text-[#80868b] leading-snug">{tpl.description}</div>
            </button>
          ))}
        </div>
      </div>
    </div>
  )
}
