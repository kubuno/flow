import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Play, Trash2, Webhook, ChevronDown, ChevronRight } from 'lucide-react'
import { Input, NumberInput, Dropdown, Checkbox, Textarea } from '@ui'
import { KeyRound } from 'lucide-react'
import type { CredentialMeta, ExprHelp, FieldDef, NodeLog, NodeMeta, WorkflowNode } from './types'
import { flowApi } from './api'
import JsonView from './JsonView'
import ExpressionField from './ExpressionField'

interface Props {
  node: WorkflowNode
  meta: NodeMeta | undefined
  workflowId: string
  lastLog: NodeLog | undefined
  exprHelp?: ExprHelp
  credentials?: CredentialMeta[]
  onManageCredentials?: (presetType?: string) => void
  onChange: (patch: Partial<WorkflowNode>) => void
  onDelete: () => void
}

/** Sélecteur de credential (filtré par type accepté) + bouton de gestion. */
function CredentialField({ field, value, credentials, onChange, onManage }: {
  field: FieldDef; value: unknown; credentials: CredentialMeta[]
  onChange: (v: unknown) => void; onManage?: (presetType?: string) => void
}) {
  const types = (field.credentialType ?? '').split(',').map(s => s.trim()).filter(Boolean)
  const compatible = credentials.filter(c => types.length === 0 || types.includes(c.type))
  const base = 'flex-1 bg-[#e8eaed] border border-[#dadce0] rounded px-2 py-1.5 text-sm text-[#202124] outline-none focus:border-[#e8824a]'
  return (
    <div className="flex items-center gap-1">
      <select className={base} value={typeof value === 'string' ? value : ''} onChange={e => onChange(e.target.value || null)}>
        <option value="">—</option>
        {compatible.map(c => <option key={c.id} value={c.id}>{c.name}</option>)}
      </select>
      <button type="button" onClick={() => onManage?.(types[0])} title="Gérer les identifiants"
        className="shrink-0 px-2 py-1.5 rounded border border-[#dadce0] bg-[#e8eaed] hover:bg-[#dadce0] text-[#5f6368]">
        <KeyRound size={14} />
      </button>
    </div>
  )
}

/** Rend un résultat lisible : chaîne brute (sans guillemets), objet en JSON indenté. */
function prettyResult(data: unknown): string {
  if (data == null) return ''
  if (typeof data === 'string') return data
  if (typeof data === 'number' || typeof data === 'boolean') return String(data)
  try { return JSON.stringify(data, null, 2) } catch { return String(data) }
}

function FieldInput({ field, value, exprHelp, onChange }: { field: FieldDef; value: unknown; exprHelp?: ExprHelp; onChange: (v: unknown) => void }) {
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
      return <ExpressionField value={str} onChange={v => onChange(v)} placeholder={field.placeholder} help={exprHelp} />
    default: // text
      return <Input className={base} value={str} onChange={e => onChange(e.target.value)} placeholder={field.placeholder} />
  }
}

function DataPanel({ title, data, accent }: { title: string; data: unknown; accent: string }) {
  const [open, setOpen] = useState(true)
  if (data == null) return null
  return (
    <div className="border border-[#dadce0] rounded-md overflow-hidden">
      <button onClick={() => setOpen(o => !o)} className="w-full flex items-center gap-1 px-2 py-1.5 bg-[#f8f9fa] text-left">
        {open ? <ChevronDown size={13} className="text-[#80868b]" /> : <ChevronRight size={13} className="text-[#80868b]" />}
        <span className="text-[10px] font-semibold uppercase tracking-wider" style={{ color: accent }}>{title}</span>
      </button>
      {open && (
        <div className="p-2 text-[11px] font-mono max-h-56 overflow-auto leading-relaxed">
          <JsonView data={data} />
        </div>
      )}
    </div>
  )
}

export default function NodeConfigPanel({ node, meta, workflowId, lastLog, exprHelp, credentials = [], onManageCredentials, onChange, onDelete }: Props) {
  const { t } = useTranslation('flow')
  const [testing, setTesting] = useState(false)
  const [testOut, setTestOut] = useState<{ ok: boolean; data: unknown } | null>(null)
  const [webhookPath, setWebhookPath] = useState<string | null>(null)
  const [pinned, setPinned] = useState('')
  const [showPin, setShowPin] = useState(false)
  const [showErr, setShowErr] = useState(false)

  const setConfig = (key: string, v: unknown) => onChange({ config: { ...node.config, [key]: v } })
  const setSetting = (key: string, v: unknown) => onChange({ settings: { ...(node.settings ?? {}), [key]: v } })

  const runTest = async () => {
    setTesting(true); setTestOut(null)
    let input: unknown = {}
    if (pinned.trim()) { try { input = JSON.parse(pinned) } catch { input = pinned } }
    try {
      const r = await flowApi.testNode(workflowId, node.id, input)
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

  const isTrigger = meta?.category === 'trigger'

  return (
    <div className="w-80 max-w-[85vw] h-full bg-[#ffffff] border-l border-[#dadce0] flex flex-col no-print">
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
            {f.type === 'credential'
              ? <CredentialField field={f} value={node.config[f.name]} credentials={credentials} onChange={v => setConfig(f.name, v)} onManage={onManageCredentials} />
              : <FieldInput field={f} value={node.config[f.name]} exprHelp={exprHelp} onChange={v => setConfig(f.name, v)} />}
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

        {!isTrigger && (
          <div className="border-t border-[#dadce0] pt-3 space-y-2">
            <button onClick={() => setShowPin(s => !s)} className="flex items-center gap-1 text-[11px] text-[#5f6368] hover:text-[#202124]">
              {showPin ? <ChevronDown size={12} /> : <ChevronRight size={12} />} {t('pin_test_data', { defaultValue: 'Données de test (entrée)' })}
            </button>
            {showPin && (
              <textarea
                className="w-full bg-[#e8eaed] border border-[#dadce0] rounded px-2 py-1.5 text-xs font-mono min-h-[70px] outline-none focus:border-[#e8824a]"
                value={pinned} onChange={e => setPinned(e.target.value)}
                placeholder={'{ "exemple": 1 }'}
              />
            )}
            <button onClick={runTest} disabled={testing}
              className="flex items-center gap-1.5 text-xs bg-[#e8eaed] hover:bg-[#dadce0] text-[#202124] px-3 py-1.5 rounded disabled:opacity-50">
              <Play size={13} /> {testing ? t('testing') : t('test_node')}
            </button>
          </div>
        )}

        {testOut && (
          testOut.ok
            ? <DataPanel title={t('test_result', { defaultValue: 'Résultat du test' })} data={testOut.data} accent="#1e8e3e" />
            : (
              <div className="rounded-md border border-red-200 bg-red-50 p-2">
                <div className="text-[10px] font-semibold uppercase tracking-wider mb-1 text-red-700">{t('test_error', { defaultValue: 'Erreur' })}</div>
                <pre className="text-[11px] text-[#202124] whitespace-pre-wrap break-words max-h-48 overflow-y-auto font-mono leading-relaxed">{prettyResult(testOut.data)}</pre>
              </div>
            )
        )}

        <div className="border-t border-[#dadce0] pt-3 space-y-2">
          <label className="flex items-center gap-2 text-xs text-[#5f6368]">
            <Checkbox checked={!!node.settings?.disabled} onChange={b => setSetting('disabled', b)} />
            {t('node_disabled', { defaultValue: 'Désactiver ce nœud' })}
          </label>
          <div>
            <label className="block text-xs text-[#5f6368] mb-1">{t('node_note', { defaultValue: 'Note' })}</label>
            <textarea
              className="w-full bg-[#e8eaed] border border-[#dadce0] rounded px-2 py-1.5 text-xs min-h-[44px] outline-none focus:border-[#e8824a]"
              value={node.settings?.note ?? ''} onChange={e => setSetting('note', e.target.value)}
              placeholder={t('node_note_ph', { defaultValue: 'Documentation du nœud…' })}
            />
          </div>
        </div>

        {!isTrigger && (
          <div className="border-t border-[#dadce0] pt-3">
            <button onClick={() => setShowErr(s => !s)} className="flex items-center gap-1 text-[11px] text-[#5f6368] hover:text-[#202124]">
              {showErr ? <ChevronDown size={12} /> : <ChevronRight size={12} />} {t('error_handling', { defaultValue: 'Gestion d\'erreur' })}
            </button>
            {showErr && (
              <div className="space-y-2 mt-2">
                <div>
                  <label className="block text-xs text-[#5f6368] mb-1">{t('on_error', { defaultValue: 'En cas d\'erreur' })}</label>
                  <Dropdown className="w-full" value={node.settings?.on_error ?? 'stop'} onChange={v => setSetting('on_error', v)}
                    options={[
                      { value: 'stop', label: t('on_error_stop', { defaultValue: 'Arrêter le workflow' }) },
                      { value: 'continue', label: t('on_error_continue', { defaultValue: 'Continuer (sortie { error })' }) },
                    ]} />
                </div>
                <div className="flex gap-2">
                  <div className="flex-1">
                    <label className="block text-xs text-[#5f6368] mb-1">{t('retry_max', { defaultValue: 'Réessais' })}</label>
                    <NumberInput className="w-full" value={node.settings?.retry_max ?? 0} onChange={n => setSetting('retry_max', Math.max(0, Math.min(5, n || 0)))} />
                  </div>
                  <div className="flex-1">
                    <label className="block text-xs text-[#5f6368] mb-1">{t('retry_delay', { defaultValue: 'Délai (ms)' })}</label>
                    <NumberInput className="w-full" value={node.settings?.retry_delay_ms ?? 1000} onChange={n => setSetting('retry_delay_ms', Math.max(0, n || 0))} />
                  </div>
                </div>
              </div>
            )}
          </div>
        )}

        {lastLog && (
          <div className="border-t border-[#dadce0] pt-2 space-y-2">
            <div className="text-[10px] uppercase tracking-wider text-[#80868b]">{t('last_execution')}</div>
            <div className={`text-[11px] font-medium ${lastLog.status === 'success' ? 'text-green-700' : 'text-red-700'}`}>
              {lastLog.status} {lastLog.duration_ms != null && `· ${lastLog.duration_ms} ms`}
            </div>
            {lastLog.error_message && <div className="text-[11px] text-red-700 break-words">{lastLog.error_message}</div>}
            <DataPanel title={t('data_input', { defaultValue: 'Entrée' })} data={lastLog.input_data} accent="#1a73e8" />
            <DataPanel title={t('data_output', { defaultValue: 'Sortie' })} data={lastLog.output_data} accent="#1e8e3e" />
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
