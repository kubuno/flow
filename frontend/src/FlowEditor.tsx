import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import * as Y from 'yjs'
import { Awareness } from 'y-protocols/awareness'
import { useDebouncedAutosave, prompt, useAuthStore } from '@kubuno/sdk'
import { Plus, Play, Save, Power, History, Workflow as WorkflowIcon, Loader2 } from 'lucide-react'
import { WorkspaceShell, WORKSPACE_LIGHT } from '@kubuno/sdk'
import { flowApi, streamExecution } from './api'
import type { NodeLog, NodeMeta, Workflow, WorkflowDefinition, WorkflowEdge, WorkflowNode } from './types'
import FlowCanvas, { NODE_W } from './FlowCanvas'
import NodePicker from './NodePicker'
import NodeConfigPanel from './NodeConfigPanel'
import ExecutionHistory from './ExecutionHistory'
import { useCollab } from './collab/collabProvider'
import { userColor, PresenceAvatars } from './collab/presence'

function uid(prefix: string): string {
  const r = (globalThis.crypto?.randomUUID?.() ?? Math.random().toString(36).slice(2))
  return `${prefix}_${r.slice(0, 8)}`
}

function defaultConfig(meta: NodeMeta): Record<string, unknown> {
  const cfg: Record<string, unknown> = {}
  for (const f of meta.fields) if (f.default !== undefined) cfg[f.name] = f.default
  return cfg
}

export default function FlowEditor() {
  const { t } = useTranslation('flow')
  const { id = '' } = useParams<{ id: string }>()
  const navigate = useNavigate()

  const [wf, setWf] = useState<Workflow | null>(null)
  const [nodes, setNodes] = useState<WorkflowNode[]>([])
  const [edges, setEdges] = useState<WorkflowEdge[]>([])
  const [catalog, setCatalog] = useState<NodeMeta[]>([])
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set())
  const [showPicker, setShowPicker] = useState(false)
  const [showHistory, setShowHistory] = useState(false)
  const [logs, setLogs] = useState<Map<string, NodeLog>>(new Map())
  const [running, setRunning] = useState(false)
  const [titleDraft, setTitleDraft] = useState('')
  const [dirty, setDirty] = useState(false)
  const [saving, setSaving] = useState(false)
  const stopStream = useRef<(() => void) | null>(null)

  const metas = useMemo(() => new Map(catalog.map(m => [m.type, m])), [catalog])

  // ── Collaboration temps réel (Yjs) ───────────────────────────────────────────
  // Modèle calqué sur les éditeurs Office : un Y.Doc par workflow, nœuds & arêtes
  // dans des Y.Map clés-par-id. Les mutations écrivent dans le doc ; un observateur
  // reconstruit l'état React. La salle est relayée/persistée par le core (route
  // générique `/collab/:room/sync`). Le .kbflw reste le stockage durable (autosave).
  const authUser = useAuthStore(s => s.user)
  const doc = useMemo(() => new Y.Doc(), [id])
  const awareness = useMemo(() => new Awareness(doc), [doc])
  const yNodes = useMemo(() => doc.getMap<WorkflowNode>('nodes'), [doc])
  const yEdges = useMemo(() => doc.getMap<WorkflowEdge>('edges'), [doc])
  useEffect(() => () => awareness.destroy(), [awareness])

  const defRef    = useRef<WorkflowDefinition | null>(null)
  const emptyRef  = useRef<boolean | null>(null) // null = pas encore synchronisé
  const seededRef = useRef(false)
  // L'autosave NE DOIT PAS écrire avant la 1ʳᵉ synchro collab (sinon il sauvegarde
  // l'état vide d'avant le seed → écrase le .kbflw → PERTE DE DONNÉES).
  const [ready, setReady] = useState(false)

  const seedIfNeeded = useCallback(() => {
    if (seededRef.current || emptyRef.current !== true) return
    const def = defRef.current
    if (!def) return
    seededRef.current = true
    if (yNodes.size > 0 || yEdges.size > 0) return // la salle a déjà un état
    doc.transact(() => {
      for (const n of def.nodes) yNodes.set(n.id, n)
      for (const e of def.edges) yEdges.set(e.id, e)
    })
  }, [doc, yNodes, yEdges])

  // Observateur Y.Map → état React (rendu).
  useEffect(() => {
    const sync = () => {
      setNodes(Array.from(yNodes.values()))
      setEdges(Array.from(yEdges.values()))
    }
    yNodes.observe(sync); yEdges.observe(sync); sync()
    return () => { yNodes.unobserve(sync); yEdges.unobserve(sync) }
  }, [yNodes, yEdges])

  useCollab(`flow-workflow:${id}`, doc, !!id, {
    awareness,
    onSync: (empty) => { emptyRef.current = empty; seedIfNeeded(); setReady(true) },
  })

  useEffect(() => {
    let alive = true
    Promise.all([flowApi.get(id), flowApi.nodeCatalog()]).then(([w, cat]) => {
      if (!alive) return
      setWf(w); setTitleDraft(w.name)
      defRef.current = { nodes: w.definition?.nodes ?? [], edges: w.definition?.edges ?? [] }
      setCatalog(cat)
      seedIfNeeded()
      // Repli hors-ligne : si la collab n'a pas synchronisé sous 2,5 s, amorcer
      // localement depuis le fichier pour ne jamais afficher un éditeur vide.
      setTimeout(() => {
        if (!alive) return
        if (!seededRef.current && emptyRef.current === null && yNodes.size === 0 && yEdges.size === 0) {
          emptyRef.current = true; seedIfNeeded()
        }
        setReady(true) // débloque l'autosave (au pire après le repli hors-ligne)
      }, 2500)
    }).catch(() => { /* ignore */ })
    return () => { alive = false; stopStream.current?.() }
  }, [id, yNodes, yEdges, seedIfNeeded])

  // Présence : publier notre identité (avatars/curseurs).
  useEffect(() => {
    if (!authUser) return
    awareness.setLocalStateField('user', {
      id:     authUser.id,
      name:   authUser.display_name || authUser.username || authUser.email,
      color:  userColor(authUser.id),
      avatar: authUser.avatar_url,
    })
  }, [awareness, authUser])

  // Esc ferme la palette.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') setShowPicker(false) }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  const markDirty = () => setDirty(true)

  // Suppression des arêtes touchant un nœud (helper interne, dans une transaction).
  const dropEdgesTouching = useCallback((ids: Set<string>) => {
    for (const e of [...yEdges.values()]) if (ids.has(e.source) || ids.has(e.target)) yEdges.delete(e.id)
  }, [yEdges])

  const addNode = useCallback((meta: NodeMeta) => {
    const node: WorkflowNode = {
      id: uid('node'),
      type: meta.type,
      name: meta.name,
      position: { x: 120 + yNodes.size * 30, y: 120 + yNodes.size * 20 },
      config: defaultConfig(meta),
    }
    yNodes.set(node.id, node); setSelectedIds(new Set([node.id])); setShowPicker(false); markDirty()
  }, [yNodes])

  const moveNode = useCallback((nid: string, x: number, y: number) => {
    const n = yNodes.get(nid); if (n) yNodes.set(nid, { ...n, position: { x, y } }); markDirty()
  }, [yNodes])

  const patchNode = useCallback((nid: string, patch: Partial<WorkflowNode>) => {
    const n = yNodes.get(nid); if (n) yNodes.set(nid, { ...n, ...patch }); markDirty()
  }, [yNodes])

  const deleteNode = useCallback((nid: string) => {
    doc.transact(() => { yNodes.delete(nid); dropEdgesTouching(new Set([nid])) })
    setSelectedIds(prev => { const s = new Set(prev); s.delete(nid); return s }); markDirty()
  }, [doc, yNodes, dropEdgesTouching])

  const deleteSelected = useCallback(() => {
    doc.transact(() => { for (const id of selectedIds) yNodes.delete(id); dropEdgesTouching(selectedIds) })
    setSelectedIds(new Set()); markDirty()
  }, [doc, yNodes, dropEdgesTouching, selectedIds])

  // Suppr/Backspace supprime la sélection (hors champ de saisie).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Delete' && e.key !== 'Backspace') return
      const el = e.target as HTMLElement | null
      if (el?.tagName === 'INPUT' || el?.tagName === 'TEXTAREA' || el?.isContentEditable) return
      if (selectedIds.size > 0) { e.preventDefault(); deleteSelected() }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [selectedIds, deleteSelected])

  const connect = useCallback((source: string, sourcePort: string, target: string) => {
    if ([...yEdges.values()].some(x => x.source === source && x.target === target && (x.source_port ?? 'default') === sourcePort)) return
    const edge: WorkflowEdge = { id: uid('edge'), source, target, source_port: sourcePort === 'default' ? null : sourcePort, target_port: null }
    yEdges.set(edge.id, edge); markDirty()
  }, [yEdges])

  const deleteEdge = useCallback((eid: string) => { yEdges.delete(eid); markDirty() }, [yEdges])

  // Personnalisation du tracé : points de passage (waypoints) d'une connexion.
  const setEdgeWaypoints = useCallback((eid: string, waypoints: { x: number; y: number }[]) => {
    const e = yEdges.get(eid)
    if (e) yEdges.set(eid, { ...e, waypoints: waypoints.length ? waypoints : undefined })
    markDirty()
  }, [yEdges])

  // ── Actions des menus contextuels ────────────────────────────────────────────
  const clipboard = useRef<WorkflowNode | null>(null)
  const [hasClipboard, setHasClipboard] = useState(false)

  const duplicateNode = useCallback((nid: string) => {
    const n = yNodes.get(nid); if (!n) return
    const copy: WorkflowNode = { ...structuredClone(n), id: uid('node'), name: n.name ? `${n.name} (copie)` : n.name, position: { x: n.position.x + 40, y: n.position.y + 40 } }
    yNodes.set(copy.id, copy); setSelectedIds(new Set([copy.id])); markDirty()
  }, [yNodes])

  const copyNode = useCallback((nid: string) => {
    const n = yNodes.get(nid); if (n) { clipboard.current = structuredClone(n); setHasClipboard(true) }
  }, [yNodes])

  const paste = useCallback(() => {
    const c = clipboard.current; if (!c) return
    const copy: WorkflowNode = { ...structuredClone(c), id: uid('node'), position: { x: c.position.x + 50, y: c.position.y + 50 } }
    yNodes.set(copy.id, copy); setSelectedIds(new Set([copy.id])); markDirty()
  }, [yNodes])

  const disconnectNode = useCallback((nid: string) => {
    doc.transact(() => dropEdgesTouching(new Set([nid]))); markDirty()
  }, [doc, dropEdgesTouching])

  const renameNode = useCallback(async (nid: string) => {
    const n = yNodes.get(nid); if (!n) return
    const cur = n.name ?? metas.get(n.type)?.name ?? ''
    const name = await prompt({ title: t('ctx_rename', { defaultValue: 'Renommer le nœud' }), defaultValue: cur, allowEmpty: true })
    if (name !== null) patchNode(nid, { name: name || null })
  }, [yNodes, metas, patchNode, t])

  const save = useCallback(async () => {
    if (!wf || !ready) return // ne jamais sauvegarder l'état pré-synchro (vide)
    setSaving(true)
    const definition: WorkflowDefinition = { nodes, edges }
    try {
      const updated = await flowApi.update(wf.id, { name: titleDraft, definition })
      setWf(updated); setDirty(false)
    } catch { /* ignore */ } finally { setSaving(false) }
  }, [wf, ready, nodes, edges, titleDraft])

  // Autosave fiable (debounce + flush au démontage/fermeture) — le workflow
  // (.kbflw) n'était sauvé que manuellement / avant exécution.
  // Gardé par `ready` : tant que la collab n'a pas synchronisé/amorcé, l'état
  // {nodes,edges} est vide et NE DOIT PAS être sauvegardé (sinon perte de données).
  useDebouncedAutosave({ nodes, edges }, ready && !!wf, () => { void save() })

  const run = useCallback(async () => {
    if (!wf) return
    await save()
    setLogs(new Map()); setRunning(true); setShowHistory(false)
    try {
      const { execution_id } = await flowApi.execute(wf.id)
      stopStream.current?.()
      stopStream.current = streamExecution(
        execution_id,
        (log) => setLogs(prev => { const m = new Map(prev); m.set(log.node_id, log); return m }),
        () => setRunning(false),
        () => setRunning(false),
      )
    } catch { setRunning(false) }
  }, [wf, save])

  const toggleActive = useCallback(async () => {
    if (!wf) return
    const updated = wf.status === 'active' ? await flowApi.deactivate(wf.id) : await flowApi.activate(wf.id)
    setWf(updated)
  }, [wf])

  const handleDelete = useCallback(async () => {
    if (!wf) return
    await flowApi.remove(wf.id)
    navigate('/flow')
  }, [wf, navigate])

  const handleNew = useCallback(async () => {
    const w = await flowApi.create({ name: t('new_workflow') })
    navigate(`/flow/${w.id}`)
  }, [navigate])

  const handleDuplicate = useCallback(async () => {
    if (!wf) return
    const w = await flowApi.duplicate(wf.id)
    navigate(`/flow/${w.id}`)
  }, [wf, navigate])

  const selectedId = selectedIds.size === 1 ? [...selectedIds][0] : null
  const selected = selectedId ? nodes.find(n => n.id === selectedId) ?? null : null

  // Outils principaux dans la TOOLBAR (options bar), sous la barre de menus.
  const toolbarActions = (
    <div className="flex items-center gap-1.5 px-2 w-full">
      <button onClick={() => setShowPicker(true)} className="flex items-center gap-1 text-xs text-white bg-[#e8824a] hover:bg-[#d9733b] px-2.5 py-1 rounded">
        <Plus size={14} /> {t('add_node')}
      </button>
      <div className="w-px h-5 bg-[#dadce0] mx-1" />
      <button onClick={run} disabled={running} className="flex items-center gap-1 text-xs text-[#202124] bg-[#e8eaed] hover:bg-[#dadce0] px-2.5 py-1 rounded disabled:opacity-50">
        {running ? <Loader2 size={14} className="animate-spin" /> : <Play size={14} />} {t('test')}
      </button>
      <button onClick={toggleActive} className={`flex items-center gap-1 text-xs px-2.5 py-1 rounded ${wf?.status === 'active' ? 'text-green-700 bg-green-100 hover:bg-green-200' : 'text-[#202124] bg-[#e8eaed] hover:bg-[#dadce0]'}`}>
        <Power size={14} /> {wf?.status === 'active' ? t('active') : t('activate')}
      </button>
      <button onClick={() => setShowHistory(h => !h)} className="flex items-center gap-1 text-xs text-[#202124] bg-[#e8eaed] hover:bg-[#dadce0] px-2.5 py-1 rounded">
        <History size={14} /> {t('history')}
      </button>
    </div>
  )

  // Enregistrer + avatars de présence (collaborateurs en ligne) dans la topbar.
  const topbarActions = (
    <div className="flex items-center gap-2">
      <PresenceAvatars awareness={awareness} selfClientId={awareness.clientID} />
      <button onClick={save} disabled={saving || !dirty} className="flex items-center gap-1 text-xs text-[#202124] bg-[#e8eaed] hover:bg-[#dadce0] px-2.5 py-1.5 rounded disabled:opacity-40">
        <Save size={14} /> {t('save')}
      </button>
    </div>
  )

  return (
    <WorkspaceShell
      theme={WORKSPACE_LIGHT}
      chromeless
      topbarHeight={64}
      titleIcon={<WorkflowIcon size={16} style={{ color: WORKSPACE_LIGHT.accent }} />}
      title={titleDraft}
      onTitleChange={(v) => { setTitleDraft(v); markDirty() }}
      onTitleCommit={save}
      titlePlaceholder={t('untitled')}
      saveStatus={saving ? t('saving') : dirty ? t('modified') : t('saved')}
      onBack={() => navigate('/flow')}
      onDelete={handleDelete}
      deleteTitle={t('delete_workflow')}
      deleteConfirm={{ title: t('delete_confirm_title'), message: t('delete_confirm_msg'), confirmLabel: t('delete'), variant: 'danger' }}
      menuActions={{ newLabel: t('new_workflow'), onNew: handleNew, onDuplicate: handleDuplicate }}
      topbarActions={topbarActions}
      optionsBar={toolbarActions}
      optionsBarHeight={40}
    >
      <div className="relative flex flex-1 min-w-0 min-h-0">
        <div className="flex-1 relative min-w-0">
          <FlowCanvas
            nodes={nodes} edges={edges} metas={metas}
            selectedIds={selectedIds} logs={logs}
            onSelectionChange={setSelectedIds}
            onMoveNode={moveNode}
            onConnect={connect}
            onDeleteEdge={deleteEdge}
            onSetWaypoints={setEdgeWaypoints}
            onDeleteNode={deleteNode}
            onDeleteSelected={deleteSelected}
            onDuplicateNode={duplicateNode}
            onRenameNode={renameNode}
            onCopyNode={copyNode}
            onDisconnectNode={disconnectNode}
            onPaste={paste}
            canPaste={hasClipboard}
            onRequestAddNode={() => setShowPicker(true)}
          />
          {nodes.length === 0 && (
            <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
              <div className="text-center text-[#80868b]" style={{ marginLeft: NODE_W / 2 }}>
                <WorkflowIcon size={42} className="mx-auto mb-2 opacity-40" />
                <p className="text-sm">{t('add_trigger_hint')}</p>
              </div>
            </div>
          )}
          {showPicker && <NodePicker catalog={catalog} onPick={addNode} onClose={() => setShowPicker(false)} />}
        </div>

        {selected && !showHistory && (
          <NodeConfigPanel
            node={selected} meta={metas.get(selected.type)} workflowId={id}
            lastLog={logs.get(selected.id)}
            onChange={(patch) => patchNode(selected.id, patch)}
            onDelete={() => deleteNode(selected.id)}
          />
        )}

        {showHistory && wf && (
          <ExecutionHistory workflowId={wf.id} onClose={() => setShowHistory(false)} />
        )}
      </div>
    </WorkspaceShell>
  )
}
