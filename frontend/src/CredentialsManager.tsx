import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import * as Icons from 'lucide-react'
import { X, Plus, Trash2, ArrowLeft, Search, KeyRound } from 'lucide-react'
import { flowApi } from './api'
import type { CredentialMeta, CredentialType, CredField } from './types'

function LucideIcon({ name, size = 16, color }: { name: string; size?: number; color?: string }) {
  const Cmp = (Icons as unknown as Record<string, React.ComponentType<{ size?: number; color?: string }>>)[name] ?? Icons.KeyRound
  return <Cmp size={size} color={color} />
}

const CATEGORY_LABELS: Record<string, string> = {
  generic: 'Générique', database: 'Bases de données', email: 'E-mail', ai: 'IA / LLM',
  messaging: 'Messagerie', dev: 'Dev / DevOps', productivity: 'Productivité', google: 'Google',
  microsoft: 'Microsoft', cloud: 'Cloud / Stockage', crm: 'CRM / Support', marketing: 'Marketing',
  commerce: 'Paiement / E-commerce', social: 'Réseaux sociaux', cms: 'CMS', search: 'Recherche / Vectoriel', misc: 'Divers',
}

function FieldInput({ field, value, onChange }: { field: CredField; value: unknown; onChange: (v: unknown) => void }) {
  const base = 'w-full bg-[#e8eaed] border border-[#dadce0] rounded px-2 py-1.5 text-sm text-[#202124] outline-none focus:border-[#e8824a]'
  const str = value == null ? '' : String(value)
  switch (field.type) {
    case 'password': return <input type="password" autoComplete="new-password" className={base} value={str} onChange={e => onChange(e.target.value)} placeholder={field.placeholder} />
    case 'number': return <input type="number" className={base} value={str} onChange={e => onChange(e.target.value === '' ? '' : Number(e.target.value))} placeholder={field.placeholder} />
    case 'boolean': return <input type="checkbox" checked={!!value} onChange={e => onChange(e.target.checked)} />
    case 'select': return (
      <select className={base} value={str} onChange={e => onChange(e.target.value)}>
        {field.options?.map(o => <option key={o.value} value={o.value}>{o.label}</option>)}
      </select>
    )
    case 'json': return <textarea className={base + ' font-mono min-h-[70px]'} value={str} onChange={e => onChange(e.target.value)} placeholder={field.placeholder || '{ }'} />
    default: return <input className={base} value={str} onChange={e => onChange(e.target.value)} placeholder={field.placeholder} />
  }
}

export default function CredentialsManager({ onClose, presetType, onChanged }: {
  onClose: () => void
  presetType?: string
  onChanged?: () => void
}) {
  const { t } = useTranslation('flow')
  const [types, setTypes] = useState<CredentialType[]>([])
  const [creds, setCreds] = useState<CredentialMeta[]>([])
  const [view, setView] = useState<'list' | 'pick' | 'form'>(presetType ? 'pick' : 'list')
  const [q, setQ] = useState('')
  const [sel, setSel] = useState<CredentialType | null>(null)
  const [name, setName] = useState('')
  const [data, setData] = useState<Record<string, unknown>>({})
  const [saving, setSaving] = useState(false)
  const [testing, setTesting] = useState(false)
  const [testResult, setTestResult] = useState<{ ok: boolean | null; message: string } | null>(null)

  const refresh = () => flowApi.credentials().then(setCreds).catch(() => {})
  useEffect(() => {
    flowApi.credentialTypes().then(ts => {
      setTypes(ts)
      if (presetType) {
        const found = ts.find(x => x.type === presetType)
        if (found) startCreate(found)
      }
    }).catch(() => {})
    refresh()
  }, []) // eslint-disable-line react-hooks/exhaustive-deps

  const typeLabel = (id: string) => types.find(x => x.type === id)?.name ?? id
  const typeIcon = (id: string) => types.find(x => x.type === id)?.icon ?? 'KeyRound'

  const startCreate = (ct: CredentialType) => {
    setSel(ct); setName(ct.name); setTestResult(null)
    const init: Record<string, unknown> = {}
    for (const f of ct.fields) if (f.default !== undefined) init[f.name] = f.default
    setData(init); setView('form')
  }

  // Build the (coerced) payload sent to the API for both save and test.
  const buildPayload = (): Record<string, unknown> => {
    const payload: Record<string, unknown> = {}
    if (!sel) return payload
    for (const f of sel.fields) {
      let v = data[f.name]
      if (f.type === 'json' && typeof v === 'string' && v.trim()) { try { v = JSON.parse(v) } catch { /* keep string */ } }
      if (v !== undefined && v !== '') payload[f.name] = v
    }
    return payload
  }

  const runTest = async () => {
    if (!sel) return
    setTesting(true); setTestResult(null)
    try {
      const r = await flowApi.testCredential({ type: sel.type, data: buildPayload() })
      setTestResult(r)
    } catch (e) {
      setTestResult({ ok: false, message: String(e) })
    } finally { setTesting(false) }
  }

  const grouped = useMemo(() => {
    const filtered = types.filter(c => !q || c.name.toLowerCase().includes(q.toLowerCase()) || c.type.toLowerCase().includes(q.toLowerCase()))
    const m = new Map<string, CredentialType[]>()
    for (const c of filtered) { const a = m.get(c.category) ?? []; a.push(c); m.set(c.category, a) }
    return m
  }, [types, q])

  const save = async () => {
    if (!sel || !name.trim()) return
    setSaving(true)
    try {
      await flowApi.createCredential({ name: name.trim(), type: sel.type, data: buildPayload() })
      onChanged?.()
      await refresh()
      setView('list')
    } catch { /* ignore */ } finally { setSaving(false) }
  }

  const remove = async (id: string) => {
    await flowApi.deleteCredential(id).catch(() => {})
    onChanged?.(); refresh()
  }

  const orderedCats = Object.keys(CATEGORY_LABELS).filter(c => grouped.has(c))

  return (
    <div className="fixed inset-0 z-50 bg-black/40 flex items-center justify-center p-6" onClick={onClose}>
      <div className="bg-white rounded-xl shadow-2xl w-full max-w-xl max-h-[82vh] flex flex-col" onClick={e => e.stopPropagation()}>
        <div className="flex items-center justify-between px-4 py-3 border-b border-[#dadce0]">
          <div className="flex items-center gap-2">
            {view !== 'list' && <button onClick={() => setView(view === 'form' ? 'pick' : 'list')} className="text-[#80868b] hover:text-[#202124]"><ArrowLeft size={16} /></button>}
            <KeyRound size={16} className="text-[#e8824a]" />
            <h2 className="text-sm font-semibold text-[#202124]">
              {view === 'list' ? t('credentials_title', { defaultValue: 'Identifiants (credentials)' })
                : view === 'pick' ? t('cred_choose_type', { defaultValue: 'Choisir un type' })
                : sel?.name}
            </h2>
          </div>
          <button onClick={onClose} className="text-[#80868b] hover:text-[#202124]"><X size={18} /></button>
        </div>

        {/* LIST */}
        {view === 'list' && (
          <div className="flex-1 overflow-y-auto p-3">
            <button onClick={() => setView('pick')} className="w-full mb-3 flex items-center justify-center gap-1.5 text-sm text-white bg-[#e8824a] hover:bg-[#d9733b] py-2 rounded-lg">
              <Plus size={15} /> {t('cred_new', { defaultValue: 'Nouvel identifiant' })}
            </button>
            {creds.length === 0 && <div className="text-center text-[#80868b] text-sm py-8">{t('cred_empty', { defaultValue: 'Aucun identifiant enregistré.' })}</div>}
            <div className="space-y-1">
              {creds.map(c => (
                <div key={c.id} className="flex items-center gap-2 px-2 py-2 rounded hover:bg-[#f1f3f4]">
                  <span className="w-7 h-7 rounded bg-[#e8eaed] flex items-center justify-center text-[#5f6368] flex-shrink-0"><LucideIcon name={typeIcon(c.type)} size={15} /></span>
                  <span className="min-w-0 flex-1">
                    <span className="block text-sm text-[#202124] truncate">{c.name}</span>
                    <span className="block text-[11px] text-[#80868b] truncate">{typeLabel(c.type)}</span>
                  </span>
                  <button onClick={() => remove(c.id)} className="text-red-500 hover:text-red-700 p-1"><Trash2 size={15} /></button>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* TYPE PICKER */}
        {view === 'pick' && (
          <div className="flex-1 overflow-y-auto">
            <div className="p-2 border-b border-[#dadce0] sticky top-0 bg-white">
              <div className="flex items-center gap-2 bg-[#e8eaed] rounded px-2 py-1.5">
                <Search size={14} className="text-[#80868b]" />
                <input autoFocus value={q} onChange={e => setQ(e.target.value)} placeholder={t('search_placeholder', { defaultValue: 'Rechercher…' })} className="bg-transparent text-sm outline-none w-full" />
              </div>
            </div>
            <div className="p-2 space-y-3">
              {orderedCats.map(cat => (
                <div key={cat}>
                  <div className="text-[10px] uppercase tracking-wider text-[#80868b] px-1 mb-1">{CATEGORY_LABELS[cat]}</div>
                  <div className="grid grid-cols-2 gap-1">
                    {grouped.get(cat)!.map(ct => (
                      <button key={ct.type} onClick={() => startCreate(ct)} className="flex items-center gap-2 px-2 py-2 rounded hover:bg-[#e8eaed] text-left">
                        <span className="w-7 h-7 rounded bg-[#e8eaed] flex items-center justify-center text-[#5f6368] flex-shrink-0"><LucideIcon name={ct.icon} size={15} /></span>
                        <span className="text-sm text-[#202124] truncate">{ct.name}</span>
                      </button>
                    ))}
                  </div>
                </div>
              ))}
              {orderedCats.length === 0 && <div className="text-center text-[#80868b] text-sm py-8">{t('no_results', { defaultValue: 'Aucun résultat' })}</div>}
            </div>
          </div>
        )}

        {/* FORM */}
        {view === 'form' && sel && (
          <>
            <div className="flex-1 overflow-y-auto p-3 space-y-3">
              <div>
                <label className="block text-xs text-[#5f6368] mb-1">{t('cred_name', { defaultValue: 'Nom de l\'identifiant' })}</label>
                <input className="w-full bg-[#e8eaed] border border-[#dadce0] rounded px-2 py-1.5 text-sm outline-none focus:border-[#e8824a]" value={name} onChange={e => setName(e.target.value)} />
              </div>
              {sel.fields.map(f => (
                <div key={f.name}>
                  <label className="block text-xs text-[#5f6368] mb-1">{f.label}{f.required && <span className="text-red-400"> *</span>}</label>
                  <FieldInput field={f} value={data[f.name]} onChange={v => setData(d => ({ ...d, [f.name]: v }))} />
                  {f.help && <p className="text-[10px] text-[#80868b] mt-1">{f.help}</p>}
                </div>
              ))}
              <p className="text-[10px] text-[#80868b]">{t('cred_encrypted', { defaultValue: '🔒 Chiffré au repos. Les valeurs ne sont jamais renvoyées en clair.' })}</p>
              {testResult && (
                <div className={`flex items-start gap-1.5 text-xs rounded-md border p-2 ${testResult.ok === true ? 'border-green-200 bg-green-50 text-green-700' : testResult.ok === false ? 'border-red-200 bg-red-50 text-red-700' : 'border-[#dadce0] bg-[#f1f3f4] text-[#5f6368]'}`}>
                  {testResult.ok === true ? <Icons.CircleCheck size={14} className="mt-px shrink-0" /> : testResult.ok === false ? <Icons.CircleX size={14} className="mt-px shrink-0" /> : <Icons.Info size={14} className="mt-px shrink-0" />}
                  <span className="break-words">{testResult.message}</span>
                </div>
              )}
            </div>
            <div className="p-3 border-t border-[#dadce0] flex items-center justify-between gap-2">
              <button onClick={runTest} disabled={testing} className="flex items-center gap-1.5 text-sm text-[#202124] bg-[#e8eaed] hover:bg-[#dadce0] px-3 py-1.5 rounded disabled:opacity-50">
                {testing ? <Icons.Loader2 size={14} className="animate-spin" /> : <Icons.Plug size={14} />} {t('cred_test', { defaultValue: 'Tester' })}
              </button>
              <div className="flex gap-2">
                <button onClick={() => setView('pick')} className="text-sm text-[#5f6368] px-3 py-1.5 rounded hover:bg-[#e8eaed]">{t('cancel', { defaultValue: 'Annuler' })}</button>
                <button onClick={save} disabled={saving || !name.trim()} className="text-sm text-white bg-[#e8824a] hover:bg-[#d9733b] px-3 py-1.5 rounded disabled:opacity-50">{t('save', { defaultValue: 'Enregistrer' })}</button>
              </div>
            </div>
          </>
        )}
      </div>
    </div>
  )
}
