import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Play, Trash2, Webhook } from 'lucide-react'
import { Input, NumberInput, Dropdown, Checkbox, Textarea } from '@ui'
import type { FieldDef, NodeLog, NodeMeta, WorkflowNode } from './types'
import { flowApi } from './api'

interface Props {
  node: WorkflowNode
  meta: NodeMeta | undefined
  workflowId: string
  lastLog: NodeLog | undefined
  onChange: (patch: Partial<WorkflowNode>) => void
  onDelete: () => void
}

/** Rend un résultat lisible : chaîne brute (sans guillemets), objet en JSON indenté. */
function prettyResult(data: unknown): string {
  if (data == null) return ''
  if (typeof data === 'string') return data
  if (typeof data === 'number' || typeof data === 'boolean') return String(data)
  try { return JSON.stringify(data, null, 2) } catch { return String(data) }
}

function FieldInput({ field, value, onChange }: { field: FieldDef; value: unknown; onChange: (v: unknown) => void }) {
  const base = 'w-full bg-[#e8eaed] border border-[#dadce0] rounded px-2 py-1.5 text-sm text-[#202124] outline-none focus:border-[#e8824a]'
  const str = value == null ? '' : typeof value === 'string' ? value : JSON.stringify(value)

  switch (field.type) {
    case 'textarea':
      return <Textarea className={base + ' h-auto min-h-[90px]'} value={str} onChange={e => onChange(e.target.value)} placeholder={field.placeholder} />
    case 'code':
      return <textarea className={base + ' font-mono min-h-[90px]'} value={str} onChange={e => onChange(e.target.value)} placeholder={field.placeholder} />
    case 'number':
      return <NumberInput className="w-full" value={typeof value === 'number' ? value : (str === '' ? 0 : Number(str))} onChange={n => onChange(Number.isNaN(n) ? null : n)} />
    case 'boolean':
      return <Checkbox checked={!!value} onChange={b => onChange(b)} />
    case 'select':
      return (
        <Dropdown
          className="w-full"
          value={str}
          onChange={v => onChange(v)}
          placeholder="—"
          options={[{ value: '', label: '—' }, ...(field.options?.map(o => ({ value: o.value, label: o.label })) ?? [])]}
        />
      )
    case 'json':
      return <textarea className={base + ' font-mono min-h-[70px]'} value={str}
        onChange={e => { try { onChange(JSON.parse(e.target.value)) } catch { onChange(e.target.value) } }}
        placeholder={field.placeholder || '{ }'} />
    case 'expression':
      return <input className={base + ' font-mono'} value={str} onChange={e => onChange(e.target.value)} placeholder={field.placeholder} />
    default: // text
      return <Input className={base} value={str} onChange={e => onChange(e.target.value)} placeholder={field.placeholder} />
  }
}

export default function NodeConfigPanel({ node, meta, workflowId, lastLog, onChange, onDelete }: Props) {
  const { t } = useTranslation('flow')
  const [testing, setTesting] = useState(false)
  const [testOut, setTestOut] = useState<{ ok: boolean; data: unknown } | null>(null)
  const [webhookPath, setWebhookPath] = useState<string | null>(null)

  const setConfig = (key: string, v: unknown) => onChange({ config: { ...node.config, [key]: v } })

  const runTest = async () => {
    setTesting(true); setTestOut(null)
    try {
      const r = await flowApi.testNode(workflowId, node.id, {})
      setTestOut({ ok: !!r.success, data: r.success ? r.output : r.error })
    } catch (e) {
      setTestOut({ ok: false, data: String(e) })
    } finally { setTesting(false) }
  }

  const getWebhook = async () => {
    try {
      const r = await flowApi.registerWebhook(workflowId, node.id)
      setWebhookPath(r.path)
    } catch { /* ignore */ }
  }

  return (
    <div className="w-80 h-full bg-[#ffffff] border-l border-[#dadce0] flex flex-col">
      <div className="px-3 py-2 border-b border-[#dadce0]">
        <div className="text-[10px] uppercase tracking-wider text-[#80868b]">{meta?.name ?? node.type}</div>
        <input
          className="w-full bg-transparent text-[#202124] text-sm font-medium outline-none mt-0.5"
          value={node.name ?? ''} placeholder={meta?.name ?? t('node_name_placeholder')}
          onChange={e => onChange({ name: e.target.value })}
        />
      </div>

      <div className="flex-1 overflow-y-auto p-3 space-y-3">
        {meta?.description && <p className="text-[11px] text-[#80868b]">{meta.description}</p>}

        {meta?.fields.map(f => (
          <div key={f.name}>
            <label className="block text-xs text-[#5f6368] mb-1">
              {f.label}{f.required && <span className="text-red-400"> *</span>}
            </label>
            <FieldInput field={f} value={node.config[f.name]} onChange={v => setConfig(f.name, v)} />
            {f.help && <p className="text-[10px] text-[#80868b] mt-1">{f.help}</p>}
          </div>
        ))}

        {node.type === 'trigger.webhook' && (
          <div>
            <button onClick={getWebhook} className="flex items-center gap-1.5 text-xs text-[#e8824a] hover:underline">
              <Webhook size={13} /> {t('generate_webhook_url')}
            </button>
            {webhookPath && <code className="block mt-1 text-[10px] text-[#80868b] break-all bg-[#e8eaed] p-1.5 rounded">{webhookPath}</code>}
          </div>
        )}

        {meta?.category !== 'trigger' && (
          <button onClick={runTest} disabled={testing}
            className="flex items-center gap-1.5 text-xs bg-[#e8eaed] hover:bg-[#dadce0] text-[#202124] px-3 py-1.5 rounded disabled:opacity-50">
            <Play size={13} /> {testing ? t('testing') : t('test_node')}
          </button>
        )}

        {testOut && (
          <div className={`rounded-md border p-2 ${testOut.ok ? 'border-green-200 bg-green-50' : 'border-red-200 bg-red-50'}`}>
            <div className={`text-[10px] font-semibold uppercase tracking-wider mb-1 ${testOut.ok ? 'text-green-700' : 'text-red-700'}`}>
              {testOut.ok ? t('test_result', { defaultValue: 'Résultat' }) : t('test_error', { defaultValue: 'Erreur' })}
            </div>
            <pre className="text-[11px] text-[#202124] whitespace-pre-wrap break-words max-h-48 overflow-y-auto font-mono leading-relaxed">{prettyResult(testOut.data)}</pre>
          </div>
        )}

        {lastLog && (
          <div className="border-t border-[#dadce0] pt-2">
            <div className="text-[10px] uppercase tracking-wider text-[#80868b] mb-1">{t('last_execution')}</div>
            <div className={`text-[11px] font-medium ${lastLog.status === 'success' ? 'text-green-700' : 'text-red-700'}`}>
              {lastLog.status} {lastLog.duration_ms != null && `· ${lastLog.duration_ms} ms`}
            </div>
            {lastLog.error_message && <div className="text-[11px] text-red-700 mt-1 break-words">{lastLog.error_message}</div>}
            {lastLog.output_data != null && (
              <pre className="text-[10px] text-[#5f6368] mt-1 whitespace-pre-wrap break-words max-h-40 overflow-y-auto font-mono leading-relaxed bg-[#f8f9fa] rounded p-1.5">{prettyResult(lastLog.output_data)}</pre>
            )}
          </div>
        )}
      </div>

      <div className="p-3 border-t border-[#dadce0]">
        <button onClick={onDelete} className="flex items-center gap-1.5 text-xs text-red-600 hover:text-red-700">
          <Trash2 size={13} /> {t('delete_node')}
        </button>
      </div>
    </div>
  )
}
