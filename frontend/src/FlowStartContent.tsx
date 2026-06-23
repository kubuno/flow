// Start content (Accueil) — reused by the home page (ModuleHome) AND by the open
// editor's backstage ("Fichier" tab). Recents + browse + Nouveau/Modèles/Import.
// Extracted from FlowDashboard so both surfaces share the exact same UI without a
// circular import between the dashboard and the editor.
import { useEffect, useRef, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { format } from 'date-fns'
import { Plus, Workflow as WorkflowIcon, Copy, Trash2, ExternalLink, Upload, LayoutTemplate } from 'lucide-react'
import { Button } from '@ui'
import type { StartPageRecentItem, MenuItem } from '@ui'
import { ModuleStartPage } from '@kubuno/drive'
import type { FileItem } from '@kubuno/drive'
import { getDateLocale } from '@kubuno/sdk'
import { flowApi } from './api'
import { parseN8n, parseMake, parseExternalWorkflow } from './flowImport'
import TemplatesModal from './TemplatesModal'
import type { WorkflowTemplate } from './templates'
import type { Workflow } from './types'

export default function FlowStartContent() {
  const { t, i18n } = useTranslation('flow')
  const navigate = useNavigate()
  const [workflows, setWorkflows] = useState<Workflow[]>([])
  const [creating, setCreating] = useState(false)
  const [showTemplates, setShowTemplates] = useState(false)

  const load = () => {
    flowApi.list().then(setWorkflows).catch(() => setWorkflows([]))
  }
  useEffect(load, [])

  const create = async () => {
    setCreating(true)
    try {
      const wf = await flowApi.create({ name: t('new_workflow') })
      navigate(`/flow/${wf.id}`)
    } catch { setCreating(false) }
  }

  const createFromTemplate = async (tpl: WorkflowTemplate) => {
    setCreating(true)
    try {
      const wf = await flowApi.create({ name: tpl.name, definition: tpl.definition })
      navigate(`/flow/${wf.id}`)
    } catch { setCreating(false); setShowTemplates(false) }
  }

  // ── Import n8n / Make ────────────────────────────────────────────────────────
  const fileRef = useRef<HTMLInputElement>(null)
  const importKind = useRef<'n8n' | 'make'>('n8n')
  const [importing, setImporting] = useState(false)
  const [importErr, setImportErr] = useState<string | null>(null)

  const triggerImport = (kind: 'n8n' | 'make') => { importKind.current = kind; setImportErr(null); fileRef.current?.click() }
  // Entries added to the file browser's native "Import" menu (next to "Import
  // files" which uploads .kbflw). → a single Import button.
  const importItems: MenuItem[] = [
    { type: 'action', label: t('import_n8n',  { defaultValue: 'Depuis n8n' }),  icon: <Upload size={15} />, onClick: () => triggerImport('n8n') },
    { type: 'action', label: t('import_make', { defaultValue: 'Depuis Make' }), icon: <Upload size={15} />, onClick: () => triggerImport('make') },
  ]

  const onImportFile = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]; e.target.value = ''
    if (!file) return
    setImporting(true); setImportErr(null)
    try {
      const text = await file.text()
      const wf = importKind.current === 'n8n'
        ? parseN8n(JSON.parse(text))
        : importKind.current === 'make'
          ? parseMake(JSON.parse(text))
          : parseExternalWorkflow(text).wf
      const created = await flowApi.create({ name: wf.name, definition: wf.definition })
      navigate(`/flow/${created.id}`)
    } catch (err) {
      setImportErr(err instanceof Error ? err.message : t('import_failed', { defaultValue: 'Import impossible' }))
      setImporting(false)
      setTimeout(() => setImportErr(null), 5000)
    }
  }

  const duplicate = async (wf: Workflow) => { await flowApi.duplicate(wf.id); load() }
  const remove    = async (wf: Workflow) => { await flowApi.remove(wf.id); setWorkflows(ws => ws.filter(w => w.id !== wf.id)) }

  // Opening a .kbflw file from the browser → editor.
  const handleOpenFile = (file: FileItem): boolean => {
    flowApi.openByFile(file.id).then(wf => navigate(`/flow/${wf.id}`)).catch(() => {})
    return true
  }

  const recentItems: StartPageRecentItem[] = workflows.slice(0, 12).map(wf => ({
    id:       wf.id,
    name:     wf.name,
    subtitle: wf.updated_at ? format(new Date(wf.updated_at), 'd MMM', { locale: getDateLocale(i18n.language) }) : undefined,
    icon:     <WorkflowIcon size={18} className="text-text-tertiary" strokeWidth={1.5} />,
    onClick:  () => navigate(`/flow/${wf.id}`),
    actions: [
      { id: 'open',  label: t('open', { defaultValue: 'Ouvrir' }), icon: <ExternalLink size={15} />, onClick: () => navigate(`/flow/${wf.id}`) },
      { id: 'dup',   label: t('duplicate'),                        icon: <Copy size={15} />,         onClick: () => duplicate(wf) },
      { id: 'trash', label: t('delete'),                           icon: <Trash2 size={15} />, danger: true, onClick: () => remove(wf) },
    ],
  }))

  return (
    <>
    <ModuleStartPage
      recentTitle={t('recent', { defaultValue: 'Récents' })}
      recentItems={recentItems}
      recentEmpty={
        <div className="flex flex-col items-center gap-2">
          <WorkflowIcon size={32} className="text-text-tertiary opacity-30" strokeWidth={1.5} />
          <p className="text-text-tertiary text-xs">{t('no_workflows')}</p>
        </div>
      }
      browse={{
        folderPathPrefix: 'Flow',
        title: 'Flow',
        fileTypeModuleId: 'flow',
        onOpenFile: handleOpenFile,
        importMenuItems: importItems,
        toolbarContent: (
          <div className="flex items-center gap-2 flex-wrap">
            <Button icon={<Plus size={15} />} onClick={create} loading={creating || importing}>
              {t('new_workflow')}
            </Button>
            <Button variant="secondary" icon={<LayoutTemplate size={15} />} onClick={() => setShowTemplates(true)}>
              {t('templates', { defaultValue: 'Modèles' })}
            </Button>
            {importErr && <span className="text-xs text-danger">{importErr}</span>}
            <input ref={fileRef} type="file" accept=".json,application/json" className="hidden" onChange={onImportFile} />
          </div>
        ),
      }}
    />
    {showTemplates && (
      <TemplatesModal busy={creating} onClose={() => setShowTemplates(false)} onPick={createFromTemplate} />
    )}
    </>
  )
}
