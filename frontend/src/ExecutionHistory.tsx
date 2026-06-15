import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { X, CheckCircle2, XCircle, Loader2, ChevronRight } from 'lucide-react'
import { flowApi } from './api'
import type { Execution, NodeLog } from './types'

function fmt(iso: string) { return new Date(iso).toLocaleString() }
function dur(ms: number | null) { return ms == null ? '–' : ms < 1000 ? `${ms}ms` : `${(ms / 1000).toFixed(2)}s` }

function StatusIcon({ s }: { s: string }) {
  if (s === 'success') return <CheckCircle2 size={15} className="text-green-400" />
  if (s === 'error' || s === 'stopped') return <XCircle size={15} className="text-red-400" />
  return <Loader2 size={15} className="text-blue-400 animate-spin" />
}

export default function ExecutionHistory({ workflowId, onClose }: { workflowId: string; onClose: () => void }) {
  const { t } = useTranslation('flow')
  const [execs, setExecs] = useState<Execution[]>([])
  const [openId, setOpenId] = useState<string | null>(null)
  const [logs, setLogs] = useState<NodeLog[]>([])

  useEffect(() => {
    flowApi.executions(workflowId).then(setExecs).catch(() => setExecs([]))
  }, [workflowId])

  const toggle = async (id: string) => {
    if (openId === id) { setOpenId(null); return }
    setOpenId(id)
    try { const d = await flowApi.executionDetail(id); setLogs(d.node_logs) } catch { setLogs([]) }
  }

  return (
    <div className="w-96 h-full bg-[#ffffff] border-l border-[#dadce0] flex flex-col">
      <div className="flex items-center justify-between px-3 py-2 border-b border-[#dadce0]">
        <span className="text-[#5f6368] text-sm font-semibold">{t('exec_history')}</span>
        <button className="text-[#80868b] hover:text-[#202124]" onClick={onClose}><X size={18} /></button>
      </div>
      <div className="flex-1 overflow-y-auto">
        {execs.length === 0 && <div className="text-center text-[#80868b] text-sm py-8">{t('no_executions')}</div>}
        {execs.map(ex => (
          <div key={ex.id} className="border-b border-[#e8eaed]">
            <button onClick={() => toggle(ex.id)} className="w-full flex items-center gap-2 px-3 py-2 hover:bg-[#e8eaed] text-left">
              <ChevronRight size={14} className={`text-[#80868b] transition-transform ${openId === ex.id ? 'rotate-90' : ''}`} />
              <StatusIcon s={ex.status} />
              <span className="flex-1 min-w-0">
                <span className="block text-xs text-[#202124]">{fmt(ex.started_at)}</span>
                <span className="block text-[10px] text-[#80868b]">{ex.trigger_source} · {t('history_nodes', { executed: ex.nodes_executed, total: ex.nodes_total })} · {dur(ex.duration_ms)}</span>
              </span>
            </button>
            {openId === ex.id && (
              <div className="px-3 pb-2 space-y-1">
                {ex.error_message && <div className="text-[11px] text-red-300">{ex.error_message}</div>}
                {logs.map(l => (
                  <div key={l.id} className="flex items-center gap-2 text-[11px] py-0.5">
                    <StatusIcon s={l.status} />
                    <span className="text-[#5f6368] flex-1 truncate">{l.node_name || l.node_type}</span>
                    <span className="text-[#80868b]">{dur(l.duration_ms)}</span>
                  </div>
                ))}
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  )
}
