import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import * as Y from 'yjs'
import { Awareness } from 'y-protocols/awareness'
import { useDebouncedAutosave, prompt, useAuthStore } from '@kubuno/sdk'
import { Plus, Play, Power, History, Workflow as WorkflowIcon, Loader2, Undo2, Redo2, KeyRound, StickyNote as StickyNoteIcon, Trash2, Copy, Star } from 'lucide-react'
import { flowApi, streamExecution } from './api'
import type { CredentialMeta, ExprHelp, NodeLog, NodeMeta, StickyNote, Workflow, WorkflowDefinition, WorkflowEdge, WorkflowNode } from './types'
import FlowCanvas, { NODE_W } from './FlowCanvas'
import NodePicker from './NodePicker'
import NodeConfigPanel from './NodeConfigPanel'
import CredentialsManager from './CredentialsManager'
import ExecutionHistory from './ExecutionHistory'
import { useCollab } from './collab/collabProvider'
import { userColor, PresenceAvatars } from './collab/presence'
import FlowStartContent from './FlowStartContent'
import { OfficeShell } from './shell/OfficeShell'
import { THEME_FLOW } from './ribbon/officeThemes'
import { useFileTab, backstageLabels, InfoPanel } from './ribbon/ModuleBackstage'
import { SaveButton } from './ribbon/SaveButton'
import type { RibbonTab } from './ribbon/types'

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
  const [notes, setNotes] = useState<StickyNote[]>([])
  const [, forceUndoTick] = useState(0)
  const [catalog, setCatalog] = useState<NodeMeta[]>([])
  const [exprHelp, setExprHelp] = useState<ExprHelp | undefined>(undefined)
  const [credentials, setCredentials] = useState<CredentialMeta[]>([])
  const [credsManager, setCredsManager] = useState<{ open: boolean; preset?: string }>({ open: false })
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
  const yNotes = useMemo(() => doc.getMap<StickyNote>('notes'), [doc])
  // Annuler/refaire : l'UndoManager pilote les Y.Map (modèle collab → undo gratuit).
  const undoMgr = useMemo(() => new Y.UndoManager([yNodes, yEdges, yNotes], { captureTimeout: 350 }), [yNodes, yEdges, yNotes])
  useEffect(() => () => { undoMgr.destroy() }, [undoMgr])
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
      for (const note of def.notes ?? []) yNotes.set(note.id, note)
    })
  }, [doc, yNodes, yEdges, yNotes])

  // Observateur Y.Map → état React (rendu).
  useEffect(() => {
    const sync = () => {
      setNodes(Array.from(yNodes.values()))
      setEdges(Array.from(yEdges.values()))
      setNotes(Array.from(yNotes.values()))
    }
    yNodes.observe(sync); yEdges.observe(sync); yNotes.observe(sync); sync()
    return () => { yNodes.unobserve(sync); yEdges.unobserve(sync); yNotes.unobserve(sync) }
  }, [yNodes, yEdges, yNotes])

  // Rafraîchir l'état des boutons annuler/refaire quand la pile change.
  useEffect(() => {
    const upd = () => forceUndoTick(x => x + 1)
    undoMgr.on('stack-item-added', upd); undoMgr.on('stack-item-popped', upd)
    return () => { undoMgr.off('stack-item-added', upd); undoMgr.off('stack-item-popped', upd) }
  }, [undoMgr])

  useCollab(`flow-workflow:${id}`, doc, !!id, {
    awareness,
    onSync: (empty) => { emptyRef.current = empty; seedIfNeeded(); setReady(true) },
  })

  useEffect(() => {
    let alive = true
    flowApi.expressionHelp().then(h => { if (alive) setExprHelp(h) }).catch(() => { /* ignore */ })
    flowApi.credentials().then(c => { if (alive) setCredentials(c) }).catch(() => { /* ignore */ })
    Promise.all([flowApi.get(id), flowApi.nodeCatalog()]).then(([w, cat]) => {
      if (!alive) return
      setWf(w); setTitleDraft(w.name)
      defRef.current = { nodes: w.definition?.nodes ?? [], edges: w.definition?.edges ?? [], notes: w.definition?.notes ?? [] }
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

  // Placement en attente quand le picker est ouvert depuis un glisser (connexion
  // vers le vide) ou depuis le bouton « + » d'une connexion (insertion).
  const pendingPlacement = useRef<
    | { kind: 'connect'; source: string; port: string; x: number; y: number }
    | { kind: 'insert'; edgeId: string }
    | null
  >(null)

  const addNode = useCallback((meta: NodeMeta) => {
    const id = uid('node')
    const place = pendingPlacement.current
    pendingPlacement.current = null

    let pos = { x: 120 + yNodes.size * 30, y: 120 + yNodes.size * 20 }
    if (place?.kind === 'connect') {
      pos = { x: Math.round(place.x), y: Math.round(place.y - 30) }
    } else if (place?.kind === 'insert') {
      const e = yEdges.get(place.edgeId)
      const s = e && yNodes.get(e.source); const tg = e && yNodes.get(e.target)
      if (s && tg) pos = { x: Math.round((s.position.x + tg.position.x) / 2), y: Math.round((s.position.y + tg.position.y) / 2) }
    }
    const node: WorkflowNode = { id, type: meta.type, name: meta.name, position: pos, config: defaultConfig(meta) }

    const mkEdge = (source: string, target: string, sourcePort: string | null): WorkflowEdge => {
      const eid = uid('edge')
      return { id: eid, source, target, source_port: sourcePort, target_port: null }
    }
    doc.transact(() => {
      yNodes.set(id, node)
      if (place?.kind === 'connect') {
        const e = mkEdge(place.source, id, place.port === 'default' ? null : place.port)
        yEdges.set(e.id, e)
      } else if (place?.kind === 'insert') {
        const orig = yEdges.get(place.edgeId)
        if (orig) {
          yEdges.delete(orig.id)
          const e1 = mkEdge(orig.source, id, orig.source_port ?? null)
          const e2 = mkEdge(id, orig.target, null)
          yEdges.set(e1.id, e1); yEdges.set(e2.id, e2)
        }
      }
    })
    setSelectedIds(new Set([id])); setShowPicker(false); markDirty()
  }, [doc, yNodes, yEdges])

  const openPickerForConnect = useCallback((source: string, port: string, x: number, y: number) => {
    pendingPlacement.current = { kind: 'connect', source, port, x, y }
    setShowPicker(true)
  }, [])
  const openPickerForInsert = useCallback((edgeId: string) => {
    pendingPlacement.current = { kind: 'insert', edgeId }
    setShowPicker(true)
  }, [])

  // ── Notes autocollantes ──────────────────────────────────────────────────────
  const NOTE_COLORS = ['#fff8c4', '#d7f0d0', '#d4e4fb', '#fbd9d3', '#ece1f9']
  const addNote = useCallback((x: number, y: number) => {
    const id = uid('note')
    const color = NOTE_COLORS[yNotes.size % NOTE_COLORS.length]
    const note: StickyNote = { id, position: { x: Math.round(x), y: Math.round(y) }, width: 180, height: 120, text: '', color }
    yNotes.set(id, note); markDirty()
  }, [yNotes])
  const moveNote = useCallback((id: string, x: number, y: number) => {
    const n = yNotes.get(id); if (n) yNotes.set(id, { ...n, position: { x, y } }); markDirty()
  }, [yNotes])
  const resizeNote = useCallback((id: string, width: number, height: number) => {
    const n = yNotes.get(id); if (n) yNotes.set(id, { ...n, width, height }); markDirty()
  }, [yNotes])
  const editNote = useCallback((id: string, text: string) => {
    const n = yNotes.get(id); if (n) yNotes.set(id, { ...n, text }); markDirty()
  }, [yNotes])
  const deleteNote = useCallback((id: string) => { yNotes.delete(id); markDirty() }, [yNotes])

  const moveNode = useCallback((nid: string, x: number, y: number) => {
    const n = yNodes.get(nid); if (n) yNodes.set(nid, { ...n, position: { x, y } }); markDirty()
  }, [yNodes])

  const patchNode = useCallback((nid: string, patch: Partial<WorkflowNode>) => {
    const n = yNodes.get(nid); if (n) yNodes.set(nid, { ...n, ...patch }); markDirty()
  }, [yNodes])

  const toggleDisabled = useCallback((nid: string) => {
    const n = yNodes.get(nid); if (!n) return
    yNodes.set(nid, { ...n, settings: { ...(n.settings ?? {}), disabled: !n.settings?.disabled } }); markDirty()
  }, [yNodes])

  const deleteNode = useCallback((nid: string) => {
    doc.transact(() => { yNodes.delete(nid); dropEdgesTouching(new Set([nid])) })
    setSelectedIds(prev => { const s = new Set(prev); s.delete(nid); return s }); markDirty()
  }, [doc, yNodes, dropEdgesTouching])

  const deleteSelected = useCallback(() => {
    doc.transact(() => { for (const id of selectedIds) yNodes.delete(id); dropEdgesTouching(selectedIds) })
    setSelectedIds(new Set()); markDirty()
  }, [doc, yNodes, dropEdgesTouching, selectedIds])

  const undo = useCallback(() => { undoMgr.undo(); markDirty() }, [undoMgr])
  const redo = useCallback(() => { undoMgr.redo(); markDirty() }, [undoMgr])

  // Raccourcis : Suppr/Backspace = supprimer la sélection ; Ctrl+Z / Ctrl+Maj+Z (ou Ctrl+Y) = annuler/refaire.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement | null
      const inField = el?.tagName === 'INPUT' || el?.tagName === 'TEXTAREA' || el?.isContentEditable
      if ((e.ctrlKey || e.metaKey) && (e.key === 'z' || e.key === 'Z')) {
        if (inField) return
        e.preventDefault()
        if (e.shiftKey) redo(); else undo()
        return
      }
      if ((e.ctrlKey || e.metaKey) && (e.key === 'y' || e.key === 'Y')) {
        if (inField) return
        e.preventDefault(); redo(); return
      }
      if (e.key !== 'Delete' && e.key !== 'Backspace') return
      if (inField) return
      if (selectedIds.size > 0) { e.preventDefault(); deleteSelected() }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [selectedIds, deleteSelected, undo, redo])

  const connect = useCallback((source: string, sourcePort: string, target: string) => {
    if ([...yEdges.values()].some(x => x.source === source && x.target === target && (x.source_port ?? 'default') === sourcePort)) return
    const edge: WorkflowEdge = { id: uid('edge'), source, target, source_port: sourcePort === 'default' ? null : sourcePort, target_port: null }
    yEdges.set(edge.id, edge); markDirty()
  }, [yEdges])

  // Branche un sous-nœud IA (modèle/mémoire/outil/parser) sur un port d'agent.
  const connectAi = useCallback((source: string, target: string, port: string) => {
    const agent = yNodes.get(target)
    const si = agent && metas.get(agent.type)?.subInputs?.find(s => s.id === port)
    doc.transact(() => {
      // Ports non « multiple » (modèle/mémoire/parser) : un seul sous-nœud → remplacer.
      if (si && !si.multiple) {
        for (const e of [...yEdges.values()]) if (e.target === target && e.target_port === port) yEdges.delete(e.id)
      } else if ([...yEdges.values()].some(e => e.source === source && e.target === target && e.target_port === port)) {
        return
      }
      const e: WorkflowEdge = { id: uid('edge'), source, target, source_port: 'ai', target_port: port }
      yEdges.set(e.id, e)
    })
    markDirty()
  }, [doc, yNodes, yEdges, metas])

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
    const definition: WorkflowDefinition = { nodes, edges, notes }
    try {
      const updated = await flowApi.update(wf.id, { name: titleDraft, definition })
      setWf(updated); setDirty(false)
    } catch { /* ignore */ } finally { setSaving(false) }
  }, [wf, ready, nodes, edges, notes, titleDraft])

  // Autosave fiable (debounce + flush au démontage/fermeture) — le workflow
  // (.kbflw) n'était sauvé que manuellement / avant exécution.
  // Gardé par `ready` : tant que la collab n'a pas synchronisé/amorcé, l'état
  // {nodes,edges} est vide et NE DOIT PAS être sauvegardé (sinon perte de données).
  useDebouncedAutosave({ nodes, edges, notes }, ready && !!wf, () => { void save() })

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

  // Add a sticky note near the canvas origin (toolbar/ribbon entry — the canvas
  // also adds notes via double-click, but the ribbon needs an explicit button).
  const addNoteAtOrigin = useCallback(() => addNote(160 + notes.length * 30, 160 + notes.length * 24), [addNote, notes.length])

  // ── Ruban façon MS Office (data-driven) — remplace la barre de menus + toolbar.
  // Onglet Accueil : Workflow (Nouveau/Dupliquer, ex-menuActions) + Édition
  // (Annuler/Refaire/Supprimer) + Exécution (Tester/Activer/Historique) +
  // Identifiants. Onglet Insertion : ajout de nœud + note. Onglet Affichage :
  // bascule de l'historique. Aucune action de l'ancienne toolbar n'est perdue.
  const ribbon: RibbonTab[] = [
    {
      id: 'home', label: t('office_bs_home', { defaultValue: 'Accueil' }),
      groups: [
        {
          id: 'workflow', label: t('rb_group_workflow', { defaultValue: 'Workflow' }),
          items: [
            { id: 'new', kind: 'button', size: 'large', icon: <Plus size={18} />, label: t('new_workflow'), onClick: handleNew },
            { id: 'dup', kind: 'button', icon: <Copy size={15} />, label: t('duplicate'), onClick: handleDuplicate, disabled: !wf },
          ],
        },
        {
          id: 'edit', label: t('rb_group_edit', { defaultValue: 'Édition' }),
          items: [
            { id: 'undo', kind: 'button', icon: <Undo2 size={15} />, label: t('undo', { defaultValue: 'Annuler' }), shortcut: 'Ctrl+Z', onClick: undo, disabled: !undoMgr.canUndo() },
            { id: 'redo', kind: 'button', icon: <Redo2 size={15} />, label: t('redo', { defaultValue: 'Refaire' }), shortcut: 'Ctrl+Maj+Z', onClick: redo, disabled: !undoMgr.canRedo() },
            { id: 'sep1', kind: 'separator' },
            { id: 'del', kind: 'button', icon: <Trash2 size={15} />, label: t('delete', { defaultValue: 'Supprimer' }), onClick: deleteSelected, disabled: selectedIds.size === 0 },
          ],
        },
        {
          id: 'exec', label: t('rb_group_exec', { defaultValue: 'Exécution' }),
          items: [
            { id: 'run', kind: 'button', size: 'large', icon: running ? <Loader2 size={18} className="animate-spin" /> : <Play size={18} />, label: t('test'), onClick: run, disabled: running },
            { id: 'activate', kind: 'toggle', icon: <Power size={15} />, label: wf?.status === 'active' ? t('active') : t('activate'), active: wf?.status === 'active', onClick: () => { void toggleActive() }, disabled: !wf },
            { id: 'history', kind: 'toggle', icon: <History size={15} />, label: t('history'), active: showHistory, onClick: () => setShowHistory(h => !h) },
          ],
        },
        {
          id: 'creds', label: t('rb_group_creds', { defaultValue: 'Identifiants' }),
          items: [
            { id: 'credentials', kind: 'button', size: 'large', icon: <KeyRound size={18} />, label: t('credentials', { defaultValue: 'Identifiants' }), onClick: () => setCredsManager({ open: true }) },
          ],
        },
      ],
    },
    {
      id: 'insert', label: t('rb_tab_insert', { defaultValue: 'Insertion' }),
      groups: [
        {
          id: 'nodes', label: t('rb_group_nodes', { defaultValue: 'Nœuds' }),
          items: [
            { id: 'add-node', kind: 'button', size: 'large', icon: <Plus size={18} />, label: t('add_node'), onClick: () => { pendingPlacement.current = null; setShowPicker(true) } },
          ],
        },
        {
          id: 'annotate', label: t('rb_group_annotate', { defaultValue: 'Annotations' }),
          items: [
            { id: 'add-note', kind: 'button', size: 'large', icon: <StickyNoteIcon size={18} />, label: t('add_note', { defaultValue: 'Note' }), onClick: addNoteAtOrigin },
          ],
        },
      ],
    },
    {
      id: 'view', label: t('rb_tab_view', { defaultValue: 'Affichage' }),
      groups: [
        {
          id: 'panels', label: t('rb_group_panels', { defaultValue: 'Panneaux' }),
          items: [
            { id: 'history-toggle', kind: 'toggle', size: 'large', icon: <History size={18} />, label: t('history'), active: showHistory, onClick: () => setShowHistory(h => !h) },
          ],
        },
      ],
    },
  ]

  // Onglet « Fichier » (backstage façon Office) — TOUJOURS en 1ʳᵉ position du ruban.
  // À l'ouverture, c'est l'onglet 'home' qui s'affiche (pas le backstage).
  const { fileTab, activeTabId, onTabChange } = useFileTab({
    theme: THEME_FLOW,
    labels: backstageLabels(t),
    startContent: <FlowStartContent />,
    defaultTab: 'home',
    doc: {
      info: (
        <InfoPanel
          title={wf?.name || t('untitled', { defaultValue: 'Sans titre' })}
          subtitle={t('title', { defaultValue: 'Flow' })}
          rows={[
            [t('office_bs_info_type', { defaultValue: 'Type' }), t('title', { defaultValue: 'Flow' })],
            [t('rb_info_nodes', { defaultValue: 'Nœuds' }), nodes.length],
            [t('rb_info_edges', { defaultValue: 'Connexions' }), edges.length],
            [t('rb_info_status', { defaultValue: 'Statut' }), wf?.status === 'active' ? t('active') : t('activate', { defaultValue: 'Inactif' })],
          ]}
        />
      ),
      onPrint: () => window.print(),
      onClose: () => navigate('/flow'),
    },
  })

  // Toggle the "starred" (favourite) flag — persisted via the partial update API.
  // This editor keeps the current workflow in local state (`wf`), so we refresh it
  // from the API response (equivalent to invalidating the current workflow query).
  const starMut = useCallback(async (next: boolean) => {
    if (!wf) return
    try {
      const updated = await flowApi.update(wf.id, { is_starred: next })
      setWf(updated)
    } catch { /* ignore */ }
  }, [wf])

  // Bouton Enregistrer placé près du titre (juste avant la corbeille) via `titleActions`.
  const saveButton = <SaveButton onSave={save} saving={saving} dirty={dirty} label={t('save')} />

  // Bouton Favori (étoile) placé après Enregistrer (ordre : Enregistrer, ⭐, puis 🗑).
  const starButton = wf ? (
    <button onClick={() => starMut(!wf.is_starred)}
      className={`p-1.5 rounded hover:bg-white/10 transition-colors flex-shrink-0 ${wf.is_starred ? 'text-warning' : 'text-white/90'}`}
      title={wf.is_starred ? t('unstar', { defaultValue: 'Retirer des favoris' }) : t('star', { defaultValue: 'Ajouter aux favoris' })}>
      <Star size={15} className={wf.is_starred ? 'fill-warning text-warning' : ''} />
    </button>
  ) : null

  // Avatars de présence (collaborateurs en ligne) à droite de la topbar.
  const topbarActions = (
    <div className="flex items-center gap-2">
      <PresenceAvatars awareness={awareness} selfClientId={awareness.clientID} />
    </div>
  )

  return (
    <OfficeShell
      ribbon={[fileTab, ...ribbon]}
      activeTabId={activeTabId}
      onTabChange={onTabChange}
      theme={THEME_FLOW}
      chromeless
      topbarHeight={64}
      titleIcon={<WorkflowIcon size={16} className="text-white/90 flex-shrink-0" />}
      title={titleDraft}
      onTitleChange={(v) => { setTitleDraft(v); markDirty() }}
      onTitleCommit={save}
      titlePlaceholder={t('untitled')}
      saveStatus={saving ? t('saving') : dirty ? t('modified') : t('saved')}
      onBack={() => navigate('/flow')}
      titleActions={<>{saveButton}{starButton}</>}
      onDelete={handleDelete}
      deleteTitle={t('delete_workflow')}
      deleteConfirm={{ title: t('delete_confirm_title'), message: t('delete_confirm_msg'), confirmLabel: t('delete'), variant: 'danger' }}
      topbarActions={topbarActions}
    >
      <div className="relative flex flex-1 min-w-0 min-h-0">
        <div className="flex-1 relative min-w-0">
          <FlowCanvas
            nodes={nodes} edges={edges} notes={notes} metas={metas}
            selectedIds={selectedIds} logs={logs}
            onSelectionChange={setSelectedIds}
            onMoveNode={moveNode}
            onConnect={connect}
            onConnectToCanvas={openPickerForConnect}
            onConnectAi={connectAi}
            onInsertOnEdge={openPickerForInsert}
            onDeleteEdge={deleteEdge}
            onSetWaypoints={setEdgeWaypoints}
            onDeleteNode={deleteNode}
            onDeleteSelected={deleteSelected}
            onDuplicateNode={duplicateNode}
            onRenameNode={renameNode}
            onCopyNode={copyNode}
            onToggleDisabled={toggleDisabled}
            onDisconnectNode={disconnectNode}
            onPaste={paste}
            canPaste={hasClipboard}
            onRequestAddNode={() => { pendingPlacement.current = null; setShowPicker(true) }}
            onAddNote={addNote}
            onMoveNote={moveNote}
            onResizeNote={resizeNote}
            onEditNote={editNote}
            onDeleteNote={deleteNote}
          />
          {nodes.length === 0 && (
            <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
              <div className="text-center text-[#80868b]" style={{ marginLeft: NODE_W / 2 }}>
                <WorkflowIcon size={42} className="mx-auto mb-2 opacity-40" />
                <p className="text-sm">{t('add_trigger_hint')}</p>
              </div>
            </div>
          )}
          {showPicker && <NodePicker catalog={catalog} onPick={addNode} onClose={() => { pendingPlacement.current = null; setShowPicker(false) }} />}
        </div>

        {selected && !showHistory && (
          <NodeConfigPanel
            node={selected} meta={metas.get(selected.type)} workflowId={id}
            lastLog={logs.get(selected.id)} exprHelp={exprHelp}
            credentials={credentials}
            onManageCredentials={(preset) => setCredsManager({ open: true, preset })}
            onChange={(patch) => patchNode(selected.id, patch)}
            onDelete={() => deleteNode(selected.id)}
          />
        )}

        {showHistory && wf && (
          <ExecutionHistory workflowId={wf.id} onClose={() => setShowHistory(false)} />
        )}
      </div>

      {credsManager.open && (
        <CredentialsManager
          presetType={credsManager.preset}
          onChanged={() => flowApi.credentials().then(setCredentials).catch(() => {})}
          onClose={() => setCredsManager({ open: false })}
        />
      )}
    </OfficeShell>
  )
}
