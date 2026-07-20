import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import * as Icons from 'lucide-react'
import clsx from 'clsx'
import { MenuDropdown, type MenuItem } from '@ui'
import type { NodeLog, NodeMeta, StickyNote, WorkflowEdge, WorkflowNode } from './types'

export const NODE_W = 210
const PORT_GAP = 22 // espacement vertical entre deux ports de sortie
const NODE_H = 96 // hauteur approximative d'un nœud (test d'intersection marquee + minimap)
// Hauteurs des bandes du box pour calculer le centre vertical réel (ports centrés).
const NODE_HEAD_H = 52, NODE_SUMMARY_H = 18, NODE_FOOTER_H = 19, NODE_DISABLED_H = 13

// Lighten (amt > 0) or darken (amt < 0) a #rrggbb colour — used for the icon
// tile gradient so each category colour gets some depth without a palette table.
// Category → existing picker-group i18n key (for the node subtitle).
const CAT_LABEL_KEY: Record<string, string> = {
  trigger: 'grp_triggers', kubuno: 'grp_kubuno', logic: 'grp_logic', external: 'grp_http',
  code: 'grp_code', ai: 'grp_ai', integration: 'grp_integrations',
}

function shade(hex: string, amt: number): string {
  const v = parseInt(hex.slice(1, 7), 16)
  const r = Math.max(0, Math.min(255, (v >> 16) + amt))
  const g = Math.max(0, Math.min(255, ((v >> 8) & 0xff) + amt))
  const b2 = Math.max(0, Math.min(255, (v & 0xff) + amt))
  return `#${((r << 16) | (g << 8) | b2).toString(16).padStart(6, '0')}`
}

function LucideIcon({ name, size = 16, color }: { name: string; size?: number; color?: string }) {
  const Cmp = (Icons as unknown as Record<string, React.ComponentType<{ size?: number; color?: string }>>)[name] ?? Icons.Box
  return <Cmp size={size} color={color} />
}

function outputs(meta: NodeMeta | undefined): string[] {
  if (!meta) return ['default']
  return meta.outputs.length > 0 ? meta.outputs.map(o => o.id) : ['default']
}

// Agents (nœuds à sous-entrées IA) : carte élargie + bande dédiée aux libellés
// des sous-ports intégrée au bas de la carte (façon n8n).
const AGENT_W = 300, NODE_SUBBAND_H = 20
function subBandH(meta: NodeMeta | undefined): number {
  return meta?.subInputs?.length ? NODE_SUBBAND_H : 0
}
/** Largeur réelle d'un nœud (les agents IA sont plus larges). */
export function nodeWidth(meta: NodeMeta | undefined): number {
  return meta?.subInputs?.length ? AGENT_W : NODE_W
}

/** Hauteur réelle du box d'un nœud (en-tête + sous-titre + pied éventuels). */
function nodeBoxHeight(n: WorkflowNode, meta: NodeMeta | undefined, hasLog: boolean): number {
  let h = NODE_HEAD_H
  if (nodeSummary(n, meta)) h += NODE_SUMMARY_H
  if (hasLog) h += NODE_FOOTER_H
  if (n.settings?.disabled) h += NODE_DISABLED_H
  return h + subBandH(meta)
}

/** Y du port d'entrée : centre vertical du corps (hors bande de sous-ports). */
function inPortYAt(n: WorkflowNode, meta: NodeMeta | undefined, hasLog: boolean): number {
  return (nodeBoxHeight(n, meta, hasLog) - subBandH(meta)) / 2
}

/** Y d'un port de sortie : centré verticalement, les ports multiples répartis autour du centre. */
function outPortYAt(n: WorkflowNode, meta: NodeMeta | undefined, portId: string, hasLog: boolean): number {
  const cy = (nodeBoxHeight(n, meta, hasLog) - subBandH(meta)) / 2
  const outs = outputs(meta)
  if (outs.length <= 1) return cy
  const idx = Math.max(0, outs.indexOf(portId))
  return cy + (idx - (outs.length - 1) / 2) * PORT_GAP
}

// ── Routage orthogonal des connecteurs (repris du sous-module Diagrams d'Office) ──
// Segments droits + coudes arrondis + sauts de croisement — plus de courbes de Bézier.
type Pt = { x: number; y: number }
const BEND_R = 10 // rayon d'arrondi des coudes
const HOP_R = 5   // rayon des sauts de croisement

/** Route orthogonale d'une sortie (à droite) vers une entrée (à gauche). */
function orthRoute(src: Pt, dst: Pt): Pt[] {
  const PAD = 28
  if (dst.x >= src.x + PAD * 2) {
    // Cible à droite : sortie → milieu vertical → entrée (équerre en Z).
    const mx = Math.round((src.x + dst.x) / 2)
    if (Math.abs(src.y - dst.y) < 1) return [src, dst]
    return [src, { x: mx, y: src.y }, { x: mx, y: dst.y }, dst]
  }
  // Cible à gauche / derrière : on sort à droite, on contourne, on rentre par la gauche.
  const my = Math.round((src.y + dst.y) / 2)
  return [
    src,
    { x: src.x + PAD, y: src.y },
    { x: src.x + PAD, y: my },
    { x: dst.x - PAD, y: my },
    { x: dst.x - PAD, y: dst.y },
    dst,
  ]
}

const dist = (p: Pt, q: Pt) => Math.hypot(q.x - p.x, q.y - p.y)

/** Chemin SVG : portions droites, coudes arrondis (quadratique) et sauts aux croisements. */
function roundedPath(pts: Pt[], hops?: Pt[][]): string {
  if (pts.length < 2) return ''
  const rad = pts.map(() => 0)
  for (let i = 1; i < pts.length - 1; i++) {
    rad[i] = Math.min(BEND_R, dist(pts[i - 1], pts[i]) / 2, dist(pts[i], pts[i + 1]) / 2)
  }
  let d = `M ${pts[0].x} ${pts[0].y}`
  for (let s = 0; s < pts.length - 1; s++) {
    const p0 = pts[s], p1 = pts[s + 1]
    const segLen = dist(p0, p1) || 1
    const dx = (p1.x - p0.x) / segLen, dy = (p1.y - p0.y) / segLen
    const startCut = s > 0 ? rad[s] : 0
    const endCut = (s + 1 < pts.length - 1) ? rad[s + 1] : 0
    const aPt = { x: p0.x + dx * startCut, y: p0.y + dy * startCut }
    const bPt = { x: p1.x - dx * endCut, y: p1.y - dy * endCut }
    const straight = dist(aPt, bPt)
    const segHops = (hops?.[s] ?? [])
      .map(P => ({ P, t: (P.x - aPt.x) * dx + (P.y - aPt.y) * dy }))
      .filter(h => h.t > HOP_R && h.t < straight - HOP_R)
      .sort((u, v) => u.t - v.t)
    for (const h of segHops) {
      const A = { x: aPt.x + dx * (h.t - HOP_R), y: aPt.y + dy * (h.t - HOP_R) }
      const B = { x: aPt.x + dx * (h.t + HOP_R), y: aPt.y + dy * (h.t + HOP_R) }
      d += ` L ${A.x} ${A.y} A ${HOP_R} ${HOP_R} 0 0 1 ${B.x} ${B.y}`
    }
    d += ` L ${bPt.x} ${bPt.y}`
    if (s + 1 < pts.length - 1) {
      // coude arrondi au sommet p1 (contrôle quadratique = le sommet)
      const p2 = pts[s + 2]
      const l2 = dist(p1, p2) || 1
      const cEnd = { x: p1.x + ((p2.x - p1.x) / l2) * rad[s + 1], y: p1.y + ((p2.y - p1.y) / l2) * rad[s + 1] }
      d += ` Q ${p1.x} ${p1.y} ${cEnd.x} ${cEnd.y}`
    }
  }
  return d
}

/** Intersection strictement intérieure de [a,b] et [c,d] (null sinon). */
function segIntersect(a: Pt, b: Pt, c: Pt, d: Pt): Pt | null {
  const r1x = b.x - a.x, r1y = b.y - a.y
  const r2x = d.x - c.x, r2y = d.y - c.y
  const den = r1x * r2y - r1y * r2x
  if (Math.abs(den) < 1e-9) return null
  const t = ((c.x - a.x) * r2y - (c.y - a.y) * r2x) / den
  const u = ((c.x - a.x) * r1y - (c.y - a.y) * r1x) / den
  const eps = 1e-3
  if (t <= eps || t >= 1 - eps || u <= eps || u >= 1 - eps) return null
  return { x: a.x + t * r1x, y: a.y + t * r1y }
}

/** Aimante un waypoint W vers une équerre alignée sur les axes (voisins P, N). */
function magnetizeRightAngle(W: Pt, P: Pt, N: Pt, thresholdDeg: number): Pt {
  const deg = (r: number) => (r * 180) / Math.PI
  const ah1 = deg(Math.atan2(Math.abs(W.y - P.y), Math.abs(W.x - P.x)))
  const av1 = deg(Math.atan2(Math.abs(W.x - N.x), Math.abs(W.y - N.y)))
  const av2 = deg(Math.atan2(Math.abs(W.x - P.x), Math.abs(W.y - P.y)))
  const ah2 = deg(Math.atan2(Math.abs(W.y - N.y), Math.abs(W.x - N.x)))
  const ok1 = ah1 <= thresholdDeg && av1 <= thresholdDeg
  const ok2 = av2 <= thresholdDeg && ah2 <= thresholdDeg
  if (!ok1 && !ok2) return W
  const e1 = { x: N.x, y: P.y }
  const e2 = { x: P.x, y: N.y }
  if (ok1 && ok2) return dist(W, e1) <= dist(W, e2) ? e1 : e2
  return ok1 ? e1 : e2
}

/** Résumé court de la configuration d'un nœud, affiché sous son titre (façon n8n). */
function nodeSummary(n: WorkflowNode, meta: NodeMeta | undefined): string {
  const c = n.config || {}
  const s = (k: string) => { const v = c[k]; return v == null ? '' : typeof v === 'string' ? v : JSON.stringify(v) }
  switch (n.type) {
    case 'external.http_request': { const m = s('method') || 'GET'; const u = s('url'); return u ? `${m} ${u}` : m }
    case 'external.ai':           return s('model') || s('provider')
    case 'logic.if':              return [s('value'), s('operator'), s('compare')].filter(Boolean).join(' ')
    case 'logic.switch':          return s('value')
    case 'logic.filter':          return [s('field'), s('operator'), s('compare')].filter(Boolean).join(' ')
    case 'logic.sort':            return [s('field'), s('order')].filter(Boolean).join(' · ')
    case 'logic.limit':           return s('count') ? `${s('count')} max` : ''
    case 'logic.set_variable':    return s('name') ? `${s('name')} = ${s('value')}` : ''
    case 'logic.calculate':       return [s('a'), s('operation'), s('b')].filter(Boolean).join(' ')
    case 'logic.datetime':        return s('operation')
    case 'logic.crypto':          return s('operation')
    case 'logic.random':          return s('operation')
    case 'logic.wait':            return s('seconds') ? `${s('seconds')} s` : ''
    case 'kubuno.mail.send':      return s('to') ? `→ ${s('to')}` : ''
    case 'kubuno.notification':   return s('title')
    case 'kubuno.tasks.create':   return s('title')
    case 'kubuno.calendar.create':return s('title')
    case 'flow.subworkflow':
    case 'flow.loop_items':       return s('workflow_id') ? '↳ sous-workflow' : ''
    case 'trigger.cron':          return s('expression')
    case 'trigger.webhook':       return 'POST /webhook'
    default: {
      // Repli générique : 1ʳᵉ valeur de champ texte/expression non vide.
      const f = meta?.fields.find(f => (f.type === 'expression' || f.type === 'text') && s(f.name))
      return f ? s(f.name) : ''
    }
  }
}

interface Viewport { tx: number; ty: number; scale: number }

interface Props {
  /** Id du workflow — clé de persistance locale du viewport (pan + zoom). */
  workflowId?: string
  nodes: WorkflowNode[]
  edges: WorkflowEdge[]
  notes: StickyNote[]
  metas: Map<string, NodeMeta>
  selectedIds: Set<string>
  logs: Map<string, NodeLog>
  onSelectionChange: (ids: Set<string>) => void
  onMoveNode: (id: string, x: number, y: number) => void
  onConnect: (source: string, sourcePort: string, target: string) => void
  onConnectToCanvas: (source: string, sourcePort: string, x: number, y: number) => void
  /** Branche un sous-nœud IA (source) sur le port `port` d'un agent (target). */
  onConnectAi: (source: string, target: string, port: string) => void
  onInsertOnEdge: (edgeId: string) => void
  onDeleteEdge: (id: string) => void
  onSetWaypoints: (edgeId: string, waypoints: { x: number; y: number }[]) => void
  onDeleteNode: (id: string) => void
  onDeleteSelected: () => void
  onDuplicateNode: (id: string) => void
  onRenameNode: (id: string) => void
  onCopyNode: (id: string) => void
  onToggleDisabled: (id: string) => void
  onDisconnectNode: (id: string) => void
  onPaste: () => void
  canPaste: boolean
  onRequestAddNode: () => void
  onAddNote: (x: number, y: number) => void
  onMoveNote: (id: string, x: number, y: number) => void
  onResizeNote: (id: string, w: number, h: number) => void
  onEditNote: (id: string, text: string) => void
  onDeleteNote: (id: string) => void
}

export default function FlowCanvas({
  workflowId,
  nodes, edges, notes, metas, selectedIds, logs, onSelectionChange, onMoveNode, onConnect, onConnectToCanvas, onConnectAi,
  onInsertOnEdge, onDeleteEdge, onSetWaypoints, onDeleteNode, onDeleteSelected, onDuplicateNode, onRenameNode,
  onCopyNode, onToggleDisabled, onDisconnectNode, onPaste, canPaste, onRequestAddNode,
  onAddNote, onMoveNote, onResizeNote, onEditNote, onDeleteNote,
}: Props) {
  const { t } = useTranslation('flow')
  const containerRef = useRef<HTMLDivElement>(null)
  // Viewport (pan + zoom) restauré depuis localStorage, par workflow — l'espace
  // de travail se rouvre là où on l'a laissé (préférence locale, non collaborative).
  const vpKey = workflowId ? `flow:vp:${workflowId}` : null
  const [vp, setVp] = useState<Viewport>(() => {
    if (vpKey) {
      try {
        const v = JSON.parse(localStorage.getItem(vpKey) || 'null') as Viewport | null
        if (v && Number.isFinite(v.tx) && Number.isFinite(v.ty) && Number.isFinite(v.scale) && v.scale >= 0.25 && v.scale <= 2.5) return v
      } catch { /* JSON invalide → défaut */ }
    }
    return { tx: 40, ty: 40, scale: 1 }
  })
  // Persistance débouncée (pan/zoom = rafales d'événements).
  useEffect(() => {
    if (!vpKey) return
    const h = setTimeout(() => { try { localStorage.setItem(vpKey, JSON.stringify(vp)) } catch { /* quota */ } }, 300)
    return () => clearTimeout(h)
  }, [vp, vpKey])
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null)
  // Glisser depuis un port de sortie pour créer une connexion (drag-to-connect).
  const [connectDrag, setConnectDrag] = useState<{ source: string; port: string; sx: number; sy: number } | null>(null)
  // Glisser une connexion IA : depuis un sous-nœud (`sub`) ou depuis un port d'agent (`agent`).
  const [aiDrag, setAiDrag] = useState<{ kind: string; sx: number; sy: number; sub?: string; agent?: { id: string; port: string } } | null>(null)
  const [ghostEnd, setGhostEnd] = useState<{ x: number; y: number } | null>(null)
  const [hoverEdge, setHoverEdge] = useState<string | null>(null)
  const [hoverNode, setHoverNode] = useState<string | null>(null)
  // Glisser de nœud(s) : déplace toute la sélection en bloc.
  const drag = useRef<{ startX: number; startY: number; items: { id: string; origX: number; origY: number }[] } | null>(null)
  const noteDrag = useRef<{ id: string; startX: number; startY: number; origX: number; origY: number } | null>(null)
  const noteResize = useRef<{ id: string; startX: number; startY: number; origW: number; origH: number } | null>(null)
  const pan = useRef<{ x: number; y: number; tx: number; ty: number } | null>(null)
  const marquee = useRef<{ sx: number; sy: number; base: Set<string> } | null>(null)
  const [mrect, setMrect] = useState<{ x: number; y: number; w: number; h: number } | null>(null)
  const [grabbing, setGrabbing] = useState(false)
  const wpDrag = useRef<{ edgeId: string; index: number } | null>(null)

  const byId = useCallback((id: string) => nodes.find(n => n.id === id), [nodes])

  const toWorld = useCallback((clientX: number, clientY: number) => {
    const r = containerRef.current!.getBoundingClientRect()
    return { x: (clientX - r.left - vp.tx) / vp.scale, y: (clientY - r.top - vp.ty) / vp.scale }
  }, [vp])

  const nodesInRect = useCallback((r: { x: number; y: number; w: number; h: number }): string[] => {
    const ids: string[] = []
    for (const n of nodes) {
      const nx = vp.tx + n.position.x * vp.scale
      const ny = vp.ty + n.position.y * vp.scale
      const nw = nodeWidth(metas.get(n.type)) * vp.scale, nh = NODE_H * vp.scale
      if (nx < r.x + r.w && nx + nw > r.x && ny < r.y + r.h && ny + nh > r.y) ids.push(n.id)
    }
    return ids
  }, [nodes, vp, metas])

  // ── Menus contextuels (clic droit) ───────────────────────────────────────────
  const ic = (name: string) => {
    const I = (Icons as unknown as Record<string, React.ComponentType<{ size?: number }>>)[name]
    return I ? <I size={15} /> : null
  }
  const openMenu = (e: React.MouseEvent, items: MenuItem[]) => {
    e.preventDefault(); e.stopPropagation()
    setMenu({ x: e.clientX, y: e.clientY, items })
  }
  const zoomBy = (f: number) => setVp(v => ({ ...v, scale: Math.min(2.5, Math.max(0.25, v.scale * f)) }))
  const resetView = () => setVp({ tx: 40, ty: 40, scale: 1 })
  const contentBounds = useCallback(() => {
    const pts = [
      ...nodes.map(n => ({ x: n.position.x, y: n.position.y, w: nodeWidth(metas.get(n.type)), h: NODE_H })),
      ...notes.map(n => ({ x: n.position.x, y: n.position.y, w: n.width, h: n.height })),
    ]
    if (!pts.length) return null
    const minX = Math.min(...pts.map(p => p.x)), minY = Math.min(...pts.map(p => p.y))
    const maxX = Math.max(...pts.map(p => p.x + p.w)), maxY = Math.max(...pts.map(p => p.y + p.h))
    return { minX, minY, maxX, maxY }
  }, [nodes, notes])
  const fitToContent = () => {
    const b = contentBounds()
    if (!b || !containerRef.current) { resetView(); return }
    const r = containerRef.current.getBoundingClientRect(), pad = 60
    const scale = Math.min(2.5, Math.max(0.25, Math.min((r.width - pad * 2) / (b.maxX - b.minX || 1), (r.height - pad * 2) / (b.maxY - b.minY || 1))))
    setVp({ tx: pad - b.minX * scale, ty: pad - b.minY * scale, scale })
  }
  const multi = selectedIds.size > 1
  const nodeMenu = (n: WorkflowNode): MenuItem[] => [
    { type: 'action', label: t('ctx_configure',  { defaultValue: 'Configurer' }),         icon: ic('Settings2'),      onClick: () => onSelectOne(n.id) },
    { type: 'action', label: t('ctx_rename',      { defaultValue: 'Renommer' }),           icon: ic('Pencil'),         onClick: () => onRenameNode(n.id) },
    { type: 'action', label: t('ctx_duplicate',   { defaultValue: 'Dupliquer' }),          shortcut: 'Ctrl+D', icon: ic('CopyPlus'), onClick: () => onDuplicateNode(n.id) },
    { type: 'action', label: t('ctx_copy',        { defaultValue: 'Copier' }),             shortcut: 'Ctrl+C', icon: ic('Copy'),     onClick: () => onCopyNode(n.id) },
    { type: 'action', label: n.settings?.disabled ? t('ctx_enable', { defaultValue: 'Activer' }) : t('ctx_disable', { defaultValue: 'Désactiver' }), icon: ic(n.settings?.disabled ? 'Power' : 'PowerOff'), onClick: () => onToggleDisabled(n.id) },
    { type: 'action', label: t('ctx_disconnect',  { defaultValue: 'Détacher les liens' }), icon: ic('Unlink'),         onClick: () => onDisconnectNode(n.id) },
    { type: 'separator' },
    (multi && selectedIds.has(n.id)
      ? { type: 'action', label: t('ctx_delete_selection', { defaultValue: 'Supprimer la sélection ({{count}})', count: selectedIds.size }), shortcut: 'Suppr', icon: ic('Trash2'), onClick: onDeleteSelected }
      : { type: 'action', label: t('ctx_delete',           { defaultValue: 'Supprimer le nœud' }),                                          shortcut: 'Suppr', icon: ic('Trash2'), onClick: () => onDeleteNode(n.id) }),
  ]
  const edgeMenu = (e: WorkflowEdge, world: { x: number; y: number }): MenuItem[] => [
    { type: 'action', label: t('ctx_insert_node', { defaultValue: 'Insérer un nœud ici' }), icon: ic('PlusCircle'), onClick: () => onInsertOnEdge(e.id) },
    { type: 'action', label: t('ctx_add_point',    { defaultValue: 'Ajouter un point' }),        icon: ic('Spline'),   onClick: () => addWaypointAt(e, world) },
    { type: 'separator' },
    { type: 'action', label: t('ctx_delete_edge',  { defaultValue: 'Supprimer la connexion' }),  icon: ic('Scissors'), onClick: () => onDeleteEdge(e.id) },
  ]
  const canvasMenu = (world: { x: number; y: number }): MenuItem[] => [
    { type: 'action', label: t('ctx_add_node',   { defaultValue: 'Ajouter un nœud' }), icon: ic('Plus'),            onClick: onRequestAddNode },
    { type: 'action', label: t('ctx_add_note',   { defaultValue: 'Ajouter une note' }), icon: ic('StickyNote'),     onClick: () => onAddNote(world.x, world.y) },
    { type: 'action', label: t('ctx_paste',      { defaultValue: 'Coller' }),          shortcut: 'Ctrl+V', disabled: !canPaste, icon: ic('ClipboardPaste'), onClick: onPaste },
    { type: 'separator' },
    { type: 'action', label: t('ctx_zoom_in',    { defaultValue: 'Zoom avant' }),      icon: ic('ZoomIn'),          onClick: () => zoomBy(1.2) },
    { type: 'action', label: t('ctx_zoom_out',   { defaultValue: 'Zoom arrière' }),    icon: ic('ZoomOut'),         onClick: () => zoomBy(0.8) },
    { type: 'action', label: t('ctx_fit',        { defaultValue: 'Ajuster à l’écran' }), icon: ic('Maximize'), onClick: fitToContent },
    { type: 'action', label: t('ctx_reset_view', { defaultValue: 'Réinitialiser la vue' }),   icon: ic('RotateCcw'), onClick: resetView },
  ]

  // ── Pan & zoom ────────────────────────────────────────────────
  // React attache `wheel` en PASSIVE (≥17) → son preventDefault est inopérant :
  // sans ce listener natif non-passif, Ctrl+molette zoomerait la PAGE entière.
  useEffect(() => {
    const el = containerRef.current
    if (!el) return
    const h = (e: WheelEvent) => e.preventDefault()
    el.addEventListener('wheel', h, { passive: false })
    return () => el.removeEventListener('wheel', h)
  }, [])

  // Molette façon n8n/Figma : Ctrl/Cmd+scroll = zoom CENTRÉ SUR LE CURSEUR
  // (le point sous la souris reste fixe), scroll simple = pan vertical,
  // Shift+scroll = pan horizontal (les navigateurs le mappent déjà sur deltaX).
  const onWheel = (e: React.WheelEvent) => {
    e.preventDefault()
    if (e.ctrlKey || e.metaKey) {
      const rect = containerRef.current?.getBoundingClientRect()
      const cx = e.clientX - (rect?.left ?? 0), cy = e.clientY - (rect?.top ?? 0)
      const factor = e.deltaY > 0 ? 0.9 : 1.1
      setVp(v => {
        const ns = Math.min(2.5, Math.max(0.25, v.scale * factor))
        const k = ns / v.scale
        return { scale: ns, tx: cx - (cx - v.tx) * k, ty: cy - (cy - v.ty) * k }
      })
      return
    }
    setVp(v => ({ ...v, tx: v.tx - e.deltaX, ty: v.ty - e.deltaY }))
  }
  const onSelectOne = (id: string) => onSelectionChange(new Set([id]))

  const onContainerPointerDown = (e: React.PointerEvent) => {
    if (e.button === 1) {
      e.preventDefault()
      setGrabbing(true)
      pan.current = { x: e.clientX, y: e.clientY, tx: vp.tx, ty: vp.ty }
      containerRef.current?.setPointerCapture?.(e.pointerId)
      return
    }
    if (e.button !== 0) return
    if (e.target !== containerRef.current && !(e.target as HTMLElement).dataset.bg) return
    const additive = e.shiftKey || e.ctrlKey || e.metaKey
    const base = additive ? new Set(selectedIds) : new Set<string>()
    if (!additive) onSelectionChange(new Set())
    const rect = containerRef.current!.getBoundingClientRect()
    marquee.current = { sx: e.clientX - rect.left, sy: e.clientY - rect.top, base }
    setMrect({ x: e.clientX - rect.left, y: e.clientY - rect.top, w: 0, h: 0 })
    containerRef.current?.setPointerCapture?.(e.pointerId)
  }

  // ── Node drag ─────────────────────────────────────────────────
  const onNodePointerDown = (e: React.PointerEvent, n: WorkflowNode) => {
    if (e.button !== 0) return
    e.stopPropagation()
    const additive = e.shiftKey || e.ctrlKey || e.metaKey
    let sel = selectedIds
    if (additive) {
      sel = new Set(selectedIds)
      if (sel.has(n.id)) { sel.delete(n.id); onSelectionChange(sel); return }
      sel.add(n.id); onSelectionChange(sel)
    } else if (!selectedIds.has(n.id)) {
      sel = new Set([n.id]); onSelectionChange(sel)
    }
    const items = nodes.filter(nn => sel.has(nn.id)).map(nn => ({ id: nn.id, origX: nn.position.x, origY: nn.position.y }))
    drag.current = { startX: e.clientX, startY: e.clientY, items }
    ;(e.target as HTMLElement).setPointerCapture?.(e.pointerId)
  }

  const onPointerMove = (e: React.PointerEvent) => {
    if (connectDrag || aiDrag) {
      setGhostEnd(toWorld(e.clientX, e.clientY))
    } else if (noteResize.current) {
      const dx = (e.clientX - noteResize.current.startX) / vp.scale
      const dy = (e.clientY - noteResize.current.startY) / vp.scale
      onResizeNote(noteResize.current.id, Math.max(120, noteResize.current.origW + dx), Math.max(80, noteResize.current.origH + dy))
    } else if (noteDrag.current) {
      const dx = (e.clientX - noteDrag.current.startX) / vp.scale
      const dy = (e.clientY - noteDrag.current.startY) / vp.scale
      onMoveNote(noteDrag.current.id, noteDrag.current.origX + dx, noteDrag.current.origY + dy)
    } else if (wpDrag.current) {
      const w = toWorld(e.clientX, e.clientY)
      const edge = edges.find(x => x.id === wpDrag.current!.edgeId)
      if (edge) {
        const idx = wpDrag.current.index
        const wps = [...(edge.waypoints ?? [])]
        // Voisins dans la polyligne routée [src, ...waypoints, dst] (index i → routé i+1).
        const route = edgeGeom.routes.get(edge.id)
        let pos: Pt = { x: w.x, y: w.y }
        if (route && route.length >= idx + 3 && !e.shiftKey) {
          pos = magnetizeRightAngle(pos, route[idx], route[idx + 2], 8)
        }
        wps[idx] = { x: Math.round(pos.x), y: Math.round(pos.y) }
        onSetWaypoints(edge.id, wps)
      }
    } else if (drag.current) {
      const dx = (e.clientX - drag.current.startX) / vp.scale
      const dy = (e.clientY - drag.current.startY) / vp.scale
      for (const it of drag.current.items) onMoveNode(it.id, it.origX + dx, it.origY + dy)
    } else if (pan.current) {
      const p = pan.current
      const cx = e.clientX, cy = e.clientY
      setVp(v => ({ ...v, tx: p.tx + (cx - p.x), ty: p.ty + (cy - p.y) }))
    } else if (marquee.current) {
      const rect = containerRef.current!.getBoundingClientRect()
      const cx = e.clientX - rect.left, cy = e.clientY - rect.top
      const r = { x: Math.min(cx, marquee.current.sx), y: Math.min(cy, marquee.current.sy), w: Math.abs(cx - marquee.current.sx), h: Math.abs(cy - marquee.current.sy) }
      setMrect(r)
      onSelectionChange(new Set([...marquee.current.base, ...nodesInRect(r)]))
    }
  }

  const onPointerUp = (e: React.PointerEvent) => {
    if (aiDrag) {
      const el = document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null
      if (aiDrag.sub) {
        // Sous-nœud → port d'agent (data-ai-in = "agentId|portId|kind").
        const slot = (el?.closest?.('[data-ai-in]') as HTMLElement | null)?.dataset.aiIn
        if (slot) { const [agentId, portId, kind] = slot.split('|'); if (kind === aiDrag.kind) onConnectAi(aiDrag.sub, agentId, portId) }
      } else if (aiDrag.agent) {
        // Port d'agent → sous-nœud (data-ai-out = "nodeId|kind").
        const out = (el?.closest?.('[data-ai-out]') as HTMLElement | null)?.dataset.aiOut
        if (out) { const [nodeId, kind] = out.split('|'); if (kind === aiDrag.kind) onConnectAi(nodeId, aiDrag.agent.id, aiDrag.agent.port) }
      }
      setAiDrag(null); setGhostEnd(null)
      return
    }
    if (connectDrag) {
      // Cible : un port d'entrée sous le curseur, sinon le vide → ouvre le picker pré-câblé.
      const el = document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null
      const inPort = el?.closest?.('[data-input]') as HTMLElement | null
      const target = inPort?.dataset.input
      if (target && target !== connectDrag.source) {
        onConnect(connectDrag.source, connectDrag.port, target)
      } else if (!el?.closest?.('[data-node]')) {
        const w = toWorld(e.clientX, e.clientY)
        onConnectToCanvas(connectDrag.source, connectDrag.port, w.x, w.y)
      }
      setConnectDrag(null); setGhostEnd(null)
    }
    drag.current = null; pan.current = null; marquee.current = null; wpDrag.current = null
    noteDrag.current = null; noteResize.current = null
    setMrect(null); setGrabbing(false)
  }

  // ── Connexion par glisser depuis un port de sortie ────────────
  const startConnect = (e: React.PointerEvent, source: string, port: string) => {
    if (e.button !== 0) return
    e.stopPropagation()
    const n = byId(source)
    const meta = n ? metas.get(n.type) : undefined
    const sx = (n?.position.x ?? 0) + nodeWidth(meta)
    const sy = (n?.position.y ?? 0) + (n ? outPortYAt(n, meta, port, !!logs.get(n.id)) : 0)
    setConnectDrag({ source, port, sx, sy })
    setGhostEnd(toWorld(e.clientX, e.clientY))
    containerRef.current?.setPointerCapture?.(e.pointerId)
  }

  // ── Connexions IA (sous-nœud ↔ port d'agent, par le bas) ──────
  // X relatif d'un port de sous-entrée d'agent (réparti sur la largeur).
  const subPortX = (idx: number, count: number, w: number = NODE_W) => (w * (idx + 1)) / (count + 1)

  const startAiFromSub = (e: React.PointerEvent, nodeId: string, kind: string) => {
    if (e.button !== 0) return
    e.stopPropagation()
    const n = byId(nodeId)
    setAiDrag({ kind, sub: nodeId, sx: (n?.position.x ?? 0) + nodeWidth(n ? metas.get(n.type) : undefined) / 2, sy: n?.position.y ?? 0 })
    setGhostEnd(toWorld(e.clientX, e.clientY))
    containerRef.current?.setPointerCapture?.(e.pointerId)
  }
  const startAiFromAgent = (e: React.PointerEvent, agentId: string, portId: string, kind: string, px: number, py: number) => {
    if (e.button !== 0) return
    e.stopPropagation()
    setAiDrag({ kind, agent: { id: agentId, port: portId }, sx: px, sy: py })
    setGhostEnd(toWorld(e.clientX, e.clientY))
    containerRef.current?.setPointerCapture?.(e.pointerId)
  }

  // ── Rendu des arêtes (routage orthogonal façon Diagrams) ──────
  const edgeEndpoints = useCallback((e: WorkflowEdge): { src: Pt; dst: Pt } | null => {
    const s = byId(e.source); const t = byId(e.target)
    if (!s || !t) return null
    const sm = metas.get(s.type), tm = metas.get(t.type)
    const src = { x: s.position.x + nodeWidth(sm), y: s.position.y + outPortYAt(s, sm, e.source_port ?? 'default', !!logs.get(s.id)) }
    const dst = { x: t.position.x, y: t.position.y + inPortYAt(t, tm, !!logs.get(t.id)) }
    return { src, dst }
  }, [byId, metas, logs])

  // Polylignes routées + sauts de croisement (le connecteur le plus récent enjambe).
  const edgeGeom = useMemo(() => {
    const routes = new Map<string, Pt[]>()
    for (const e of edges) {
      // Les arêtes IA (sous-nœud → agent) sont tracées séparément en pointillés :
      // on les exclut du routage orthogonal normal (sinon double tracé en trait plein).
      if (metas.get(byId(e.source)?.type ?? '')?.aiOutput) continue
      const ep = edgeEndpoints(e)
      if (!ep) continue
      const pts = e.waypoints?.length ? [ep.src, ...e.waypoints, ep.dst] : orthRoute(ep.src, ep.dst)
      routes.set(e.id, pts)
    }
    const hops = new Map<string, Pt[][]>()
    const ids = edges.map(e => e.id).filter(id => routes.has(id))
    for (let i = 0; i < ids.length; i++) {
      const pi = routes.get(ids[i])!
      const h: Pt[][] = []
      for (let s = 0; s < pi.length - 1; s++) {
        const cross: Pt[] = []
        for (let j = 0; j < i; j++) {
          const pj = routes.get(ids[j])!
          for (let t = 0; t < pj.length - 1; t++) {
            const X = segIntersect(pi[s], pi[s + 1], pj[t], pj[t + 1])
            if (X) cross.push(X)
          }
        }
        h[s] = cross
      }
      hops.set(ids[i], h)
    }
    return { routes, hops }
  }, [edges, edgeEndpoints])

  function distToSeg(p: Pt, a: Pt, b: Pt): number {
    const dx = b.x - a.x, dy = b.y - a.y
    const len2 = dx * dx + dy * dy || 1
    let tt = ((p.x - a.x) * dx + (p.y - a.y) * dy) / len2
    tt = Math.max(0, Math.min(1, tt))
    const px = a.x + tt * dx, py = a.y + tt * dy
    return Math.hypot(p.x - px, p.y - py)
  }

  const addWaypointAt = (e: WorkflowEdge, world: Pt) => {
    const pts = edgeGeom.routes.get(e.id); if (!pts) return
    let best = 0, bestD = Infinity
    for (let i = 0; i < pts.length - 1; i++) {
      const dd = distToSeg(world, pts[i], pts[i + 1])
      if (dd < bestD) { bestD = dd; best = i }
    }
    const wps = [...(e.waypoints ?? [])]
    wps.splice(best, 0, { x: Math.round(world.x), y: Math.round(world.y) })
    onSetWaypoints(e.id, wps)
  }

  const removeWaypoint = (e: WorkflowEdge, index: number) => {
    const wps = [...(e.waypoints ?? [])]
    wps.splice(index, 1)
    onSetWaypoints(e.id, wps)
  }

  // ── Minimap ───────────────────────────────────────────────────
  const MM_W = 168, MM_H = 116, MM_PAD = 8
  const renderMinimap = () => {
    const b = contentBounds()
    if (!b) return null
    const bw = (b.maxX - b.minX) || 1, bh = (b.maxY - b.minY) || 1
    const scale = Math.min((MM_W - MM_PAD * 2) / bw, (MM_H - MM_PAD * 2) / bh)
    const ox = MM_PAD - b.minX * scale, oy = MM_PAD - b.minY * scale
    const jump = (e: React.MouseEvent) => {
      const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
      const wx = (e.clientX - r.left - ox) / scale, wy = (e.clientY - r.top - oy) / scale
      const cont = containerRef.current!.getBoundingClientRect()
      setVp(v => ({ ...v, tx: cont.width / 2 - wx * v.scale, ty: cont.height / 2 - wy * v.scale }))
    }
    return (
      <div className="absolute bottom-3 left-3 bg-white/90 border border-[#dadce0] rounded-lg shadow-sm overflow-hidden"
        style={{ width: MM_W, height: MM_H }} onPointerDown={e => e.stopPropagation()}>
        <svg width={MM_W} height={MM_H} className="cursor-pointer" onClick={jump}>
          {notes.map(n => (
            <rect key={n.id} x={ox + n.position.x * scale} y={oy + n.position.y * scale}
              width={n.width * scale} height={n.height * scale} fill={n.color} opacity={0.5} rx={1} />
          ))}
          {nodes.map(n => {
            const meta = metas.get(n.type)
            return <rect key={n.id} x={ox + n.position.x * scale} y={oy + n.position.y * scale}
              width={nodeWidth(meta) * scale} height={NODE_H * scale} fill={meta?.color ?? '#80868b'} rx={1.5} />
          })}
        </svg>
      </div>
    )
  }

  return (
    <div
      ref={containerRef}
      data-bg="1"
      className={clsx('relative w-full h-full overflow-hidden bg-[#f1f3f4]', grabbing ? 'cursor-grabbing' : 'cursor-default')}
      style={{ backgroundImage: 'radial-gradient(#c4c7cc 1px, transparent 1px)', backgroundSize: `${24 * vp.scale}px ${24 * vp.scale}px`, backgroundPosition: `${vp.tx}px ${vp.ty}px` }}
      onWheel={onWheel}
      onPointerDown={onContainerPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerLeave={onPointerUp}
      onAuxClick={e => { if (e.button === 1) e.preventDefault() }}
      onContextMenu={e => openMenu(e, canvasMenu(toWorld(e.clientX, e.clientY)))}
    >
      <div style={{ position: 'absolute', transform: `translate(${vp.tx}px, ${vp.ty}px) scale(${vp.scale})`, transformOrigin: '0 0' }}>
        {/* Notes autocollantes (derrière les nœuds) */}
        {notes.map(note => (
          <div key={note.id} className="absolute rounded-md shadow-sm flex flex-col"
            style={{ left: note.position.x, top: note.position.y, width: note.width, height: note.height, background: note.color, border: '1px solid rgba(0,0,0,0.08)' }}
            onContextMenu={e => openMenu(e, [
              { type: 'action', label: t('ctx_delete_note', { defaultValue: 'Supprimer la note' }), icon: ic('Trash2'), onClick: () => onDeleteNote(note.id) },
            ])}
          >
            <div className="h-5 cursor-grab flex items-center px-1.5 shrink-0" style={{ borderBottom: '1px solid rgba(0,0,0,0.06)' }}
              onPointerDown={e => {
                if (e.button !== 0) return
                e.stopPropagation()
                noteDrag.current = { id: note.id, startX: e.clientX, startY: e.clientY, origX: note.position.x, origY: note.position.y }
                ;(e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId)
              }}
            >
              <Icons.GripHorizontal size={12} className="text-black/30" />
            </div>
            <textarea
              className="flex-1 w-full resize-none bg-transparent outline-none px-2 py-1 text-[12px] text-[#3c4043] leading-snug"
              value={note.text}
              placeholder={t('note_placeholder', { defaultValue: 'Écrire une note…' })}
              onChange={e => onEditNote(note.id, e.target.value)}
              onPointerDown={e => e.stopPropagation()}
            />
            <div className="absolute bottom-0 right-0 w-3 h-3 cursor-nwse-resize"
              onPointerDown={e => {
                if (e.button !== 0) return
                e.stopPropagation()
                noteResize.current = { id: note.id, startX: e.clientX, startY: e.clientY, origW: note.width, origH: note.height }
                ;(e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId)
              }}
            >
              <Icons.GripVertical size={10} className="text-black/25 rotate-45" />
            </div>
          </div>
        ))}

        {/* Arêtes */}
        <svg style={{ position: 'absolute', overflow: 'visible', pointerEvents: 'none', left: 0, top: 0 }}>
          {edges.map(e => {
            const pts = edgeGeom.routes.get(e.id)
            if (!pts) return null
            const d = roundedPath(pts, edgeGeom.hops.get(e.id))
            const log = logs.get(e.target)
            const color = log?.status === 'success' ? '#1e8e3e' : log?.status === 'error' ? '#d93025' : '#5f6368'
            // Point milieu (insertion du bouton +).
            const mi = Math.floor((pts.length - 1) / 2)
            const mid = pts[mi]
            const midNext = pts[mi + 1] ?? mid
            const mx = (mid.x + midNext.x) / 2, my = (mid.y + midNext.y) / 2
            return (
              <g key={e.id} onPointerEnter={() => setHoverEdge(e.id)} onPointerLeave={() => setHoverEdge(h => h === e.id ? null : h)}>
                <path d={d} stroke={color} strokeWidth={2} fill="none" />
                <path
                  d={d} stroke="transparent" strokeWidth={16} fill="none"
                  style={{ pointerEvents: 'stroke', cursor: 'copy' }}
                  onDoubleClick={ev => { ev.stopPropagation(); addWaypointAt(e, toWorld(ev.clientX, ev.clientY)) }}
                  onContextMenu={ev => openMenu(ev, edgeMenu(e, toWorld(ev.clientX, ev.clientY)))}
                />
                {/* Bouton + (insérer un nœud sur la connexion) — au survol. */}
                {hoverEdge === e.id && (
                  <g style={{ pointerEvents: 'all', cursor: 'pointer' }}
                    onClick={ev => { ev.stopPropagation(); onInsertOnEdge(e.id) }}>
                    <circle cx={mx} cy={my} r={9} fill="#e8824a" />
                    <path d={`M ${mx - 4} ${my} H ${mx + 4} M ${mx} ${my - 4} V ${my + 4}`} stroke="#fff" strokeWidth={2} />
                  </g>
                )}
                {(e.waypoints ?? []).map((wp, i) => (
                  <circle
                    key={i} cx={wp.x} cy={wp.y} r={5}
                    fill="#ffffff" stroke="#e8824a" strokeWidth={2}
                    style={{ pointerEvents: 'all', cursor: 'grab' }}
                    onPointerDown={ev => {
                      if (ev.button !== 0) return
                      ev.stopPropagation()
                      wpDrag.current = { edgeId: e.id, index: i }
                      ;(ev.target as SVGElement).setPointerCapture?.(ev.pointerId)
                    }}
                    onDoubleClick={ev => { ev.stopPropagation(); removeWaypoint(e, i) }}
                    onContextMenu={ev => openMenu(ev, [
                      { type: 'action', label: t('ctx_remove_point', { defaultValue: 'Retirer le point' }), icon: ic('X'), onClick: () => removeWaypoint(e, i) },
                    ])}
                  />
                ))}
              </g>
            )
          })}

          {/* Arêtes IA (sous-nœud ↔ agent) : courbe en pointillés, supprimable. */}
          {edges.map(e => {
            const s = byId(e.source), tg = byId(e.target)
            if (!s || !tg) return null
            const sm = metas.get(s.type), tm = metas.get(tg.type)
            if (!sm?.aiOutput || !tm?.subInputs) return null
            const idx = tm.subInputs.findIndex(si => si.id === (e.target_port ?? ''))
            if (idx < 0) return null
            // Extrémités = centres EXACTS des ports (cf. rendu des ports plus bas).
            const ax = tg.position.x + subPortX(idx, tm.subInputs.length, nodeWidth(tm))
            const ay = tg.position.y + nodeBoxHeight(tg, tm, !!logs.get(tg.id))
            const bx = s.position.x + nodeWidth(metas.get(s.type)) / 2, by = s.position.y
            const k = Math.max(30, Math.abs(by - ay) / 2)
            const d = `M ${ax} ${ay} C ${ax} ${ay + k}, ${bx} ${by - k}, ${bx} ${by}`
            const log = logs.get(s.id)
            const color = log?.status === 'error' ? '#d93025' : '#9aa0a6'
            const mx = (ax + bx) / 2, my = (ay + by) / 2
            const hovered = hoverEdge === e.id
            return (
              <g key={`ai-${e.id}`} onPointerEnter={() => setHoverEdge(e.id)} onPointerLeave={() => setHoverEdge(h => h === e.id ? null : h)}>
                <path d={d} stroke={hovered ? '#6750a4' : color} strokeWidth={hovered ? 2 : 1.5} strokeDasharray="4 4" fill="none" />
                <path d={d} stroke="transparent" strokeWidth={14} fill="none" style={{ pointerEvents: 'stroke', cursor: 'pointer' }}
                  onContextMenu={ev => openMenu(ev, [
                    { type: 'action', label: t('ctx_delete_edge', { defaultValue: 'Supprimer la connexion' }), icon: ic('Scissors'), onClick: () => onDeleteEdge(e.id) },
                  ])} />
                {hovered && (
                  <g style={{ pointerEvents: 'all', cursor: 'pointer' }} onClick={ev => { ev.stopPropagation(); onDeleteEdge(e.id) }}>
                    <circle cx={mx} cy={my} r={8} fill="#d93025" />
                    <path d={`M ${mx - 3.5} ${my - 3.5} L ${mx + 3.5} ${my + 3.5} M ${mx + 3.5} ${my - 3.5} L ${mx - 3.5} ${my + 3.5}`} stroke="#fff" strokeWidth={1.6} />
                  </g>
                )}
              </g>
            )
          })}

          {/* Connexion fantôme pendant le glisser-pour-connecter */}
          {connectDrag && ghostEnd && (
            <path d={roundedPath(orthRoute({ x: connectDrag.sx, y: connectDrag.sy }, ghostEnd))} stroke="#e8824a" strokeWidth={2} strokeDasharray="5 4" fill="none" />
          )}
          {/* Fantôme de connexion IA */}
          {aiDrag && ghostEnd && (
            <path d={`M ${aiDrag.sx} ${aiDrag.sy} C ${aiDrag.sx} ${(aiDrag.sy + ghostEnd.y) / 2}, ${ghostEnd.x} ${(aiDrag.sy + ghostEnd.y) / 2}, ${ghostEnd.x} ${ghostEnd.y}`}
              stroke="#6750a4" strokeWidth={2} strokeDasharray="4 4" fill="none" />
          )}
        </svg>

        {/* Nœuds */}
        {nodes.map(n => {
          const meta = metas.get(n.type)
          const outs = outputs(meta)
          const log = logs.get(n.id)
          const aiOut = meta?.aiOutput          // sous-nœud fournisseur (modèle/mémoire/outil/parser)
          const subInputs = meta?.subInputs ?? [] // ports de sous-entrée (agent)
          const hasInput = (meta?.inputs ?? 1) > 0 && !aiOut
          const disabled = !!n.settings?.disabled
          const summary = nodeSummary(n, meta)
          const isTrigger = meta?.category === 'trigger'
          const color = meta?.color ?? '#80868b'
          const w = nodeWidth(meta)
          const ring = selectedIds.has(n.id) ? 'ring-2 ring-[#e8824a]'
            : log?.status === 'success' ? 'ring-2 ring-green-500'
            : log?.status === 'error' ? 'ring-2 ring-red-500' : ''
          // Nombre d'éléments en sortie (façon n8n : « N éléments »).
          const out = log?.output_data
          const itemCount = Array.isArray(out) ? out.length : null
          return (
            <div
              key={n.id}
              data-node={n.id}
              className={clsx('absolute select-none', disabled && 'opacity-55')}
              style={{ left: n.position.x, top: n.position.y, width: w }}
              onPointerEnter={() => setHoverNode(n.id)}
              onPointerLeave={() => setHoverNode(h => h === n.id ? null : h)}
              onPointerDown={e => onNodePointerDown(e, n)}
              onContextMenu={e => { if (!selectedIds.has(n.id)) onSelectOne(n.id); openMenu(e, nodeMenu(n)) }}
              title={n.settings?.note || undefined}
            >
              {/* Barre d'actions au survol (façon n8n). Conteneur collé au bord supérieur
                  du nœud (bottom-full, pleine largeur) → zone de survol CONTINUE : pas de
                  vide entre le nœud et les boutons, donc la barre ne disparaît plus. */}
              {hoverNode === n.id && !connectDrag && (
                <div className="absolute bottom-full left-0 right-0 flex justify-center pb-1.5 z-10"
                  onPointerDown={e => e.stopPropagation()}>
                  <div className="flex items-center gap-0.5 bg-white border border-[#dadce0] rounded-md shadow px-0.5 py-0.5">
                    <button className="p-1 text-[#5f6368] hover:bg-[#e8eaed] rounded" title={disabled ? t('ctx_enable', { defaultValue: 'Activer' }) : t('ctx_disable', { defaultValue: 'Désactiver' })} onClick={() => onToggleDisabled(n.id)}>
                      {disabled ? <Icons.Power size={14} /> : <Icons.PowerOff size={14} />}
                    </button>
                    <button className="p-1 text-[#5f6368] hover:bg-[#e8eaed] rounded" title={t('ctx_duplicate', { defaultValue: 'Dupliquer' })} onClick={() => onDuplicateNode(n.id)}>
                      <Icons.CopyPlus size={14} />
                    </button>
                    <button className="p-1 text-red-600 hover:bg-red-50 rounded" title={t('ctx_delete', { defaultValue: 'Supprimer' })} onClick={() => onDeleteNode(n.id)}>
                      <Icons.Trash2 size={14} />
                    </button>
                  </div>
                </div>
              )}

              <div className={clsx('border bg-white overflow-hidden transition-shadow duration-150',
                  isTrigger ? 'rounded-r-xl rounded-l-[26px]' : 'rounded-xl',
                  hoverNode === n.id ? 'shadow-xl' : 'shadow-md',
                  disabled ? 'border-dashed border-[#9aa0a6]' : 'border-[#dadce0]', ring)}>
                {/* En-tête : bande teintée catégorie + tuile icône en dégradé + titre + badges */}
                <div className="flex items-center gap-2.5 px-2.5 py-2" style={{ background: `${color}14` }}>
                  <span className={clsx('w-9 h-9 flex items-center justify-center shrink-0 shadow-sm', isTrigger ? 'rounded-full' : 'rounded-[10px]')}
                    style={{ background: `linear-gradient(135deg, ${shade(color, 26)}, ${color} 55%, ${shade(color, -24)})` }}>
                    <LucideIcon name={meta?.icon ?? 'Box'} size={18} color="#fff" />
                  </span>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-1">
                      <span className="text-[13px] font-semibold text-[#202124] truncate">{n.name || meta?.name || n.type}</span>
                      {n.settings?.on_error === 'continue' && <Icons.ShieldCheck size={11} className="text-[#1e8e3e] shrink-0" />}
                      {!!n.settings?.retry_max && <Icons.RefreshCw size={11} className="text-[#80868b] shrink-0" />}
                      {n.settings?.note && <Icons.StickyNote size={11} className="text-[#f9ab00] shrink-0" />}
                    </div>
                    {/* Sous-titre uniquement s'il apporte une info (nom personnalisé ≠ type). */}
                    {(() => { const sub = meta?.name ?? n.type; const main = n.name || sub
                      const catKey = CAT_LABEL_KEY[meta?.category ?? '']
                      return main !== sub
                        ? <div className="text-[10px] text-[#80868b] truncate">{sub}</div>
                        : catKey ? <div className="text-[10px] truncate" style={{ color: shade(color, -30) }}>{t(catKey)}</div> : null })()}
                  </div>
                </div>
                {/* Sous-titre : résumé de la configuration */}
                {summary && (
                  <div className="px-2.5 pb-1.5 -mt-0.5 text-[10px] text-[#5f6368] truncate">{summary}</div>
                )}
                {/* Pied d'exécution : statut + durée + nombre d'éléments */}
                {log && (
                  <div className={clsx('flex items-center gap-1.5 px-2.5 py-1 text-[10px] border-t',
                    log.status === 'success' ? 'bg-green-50 border-green-100 text-green-700' : 'bg-red-50 border-red-100 text-red-700')}>
                    {log.status === 'success' ? <Icons.Check size={11} /> : <Icons.X size={11} />}
                    {log.duration_ms != null && <span>{log.duration_ms} ms</span>}
                    {itemCount != null && <span className="text-[#5f6368]">· {itemCount} {t('items', { defaultValue: 'éléments' })}</span>}
                  </div>
                )}
                {disabled && (
                  <div className="px-2.5 py-0.5 text-[9px] uppercase tracking-wider text-[#9aa0a6] border-t border-dashed border-[#dadce0]">{t('disabled', { defaultValue: 'Désactivé' })}</div>
                )}
                {/* Bande des sous-ports IA (agents) : libellés intégrés au bas de la
                    carte, chaque libellé centré au-dessus de son losange. */}
                {subInputs.length > 0 && (
                  <div className="relative border-t" style={{ height: NODE_SUBBAND_H, background: `${color}0d`, borderColor: `${color}26` }}>
                    {subInputs.map((si, i) => (
                      <span key={si.id} className="absolute -translate-x-1/2 text-[8px] leading-none text-[#5f6368] whitespace-nowrap"
                        style={{ left: subPortX(i, subInputs.length, w), top: 2 }}>
                        {si.label}{si.required && <span className="text-red-400">*</span>}
                      </span>
                    ))}
                  </div>
                )}
              </div>

              {/* Port d'entrée — cible du glisser-pour-connecter (data-input).
                  Anneau creux quand libre, rempli couleur catégorie quand connecté. */}
              {hasInput && (() => { const conn = edges.some(ed => ed.target === n.id && !ed.target_port)
                return (
                  <div
                    data-input={n.id}
                    className={clsx('absolute -left-2 w-3.5 h-3.5 rounded-full border-2 cursor-crosshair shadow-sm',
                      connectDrag && connectDrag.source !== n.id ? 'scale-125 border-[#e8824a] bg-[#e8824a]' : !conn && 'bg-white border-[#9aa0a6] hover:border-[#e8824a]')}
                    style={{ top: inPortYAt(n, meta, !!log) - 7, transition: 'transform .1s',
                             ...(conn && !(connectDrag && connectDrag.source !== n.id) ? { background: color, borderColor: '#fff' } : {}) }}
                    title={t('port_in')}
                  />
                ) })()}
              {/* Ports de sortie données — masqués pour les sous-nœuds IA. */}
              {!aiOut && outs.map((port, i) => { const conn = edges.some(ed => ed.source === n.id && (ed.source_port ?? 'default') === port)
                const dragging = connectDrag?.source === n.id && connectDrag?.port === port
                return (
                  <div key={port} className="absolute -right-2 flex items-center" style={{ top: outPortYAt(n, meta, port, !!log) - 7 }}>
                    {outs.length > 1 && (
                      <span className="absolute right-4 text-[9px] text-[#5f6368] whitespace-nowrap bg-white/90 border border-[#e8eaed] rounded-full px-1.5 py-px shadow-sm">
                        {meta?.outputs[i]?.label}
                      </span>
                    )}
                    <div
                      className={clsx('w-3.5 h-3.5 rounded-full border-2 cursor-crosshair shadow-sm',
                        dragging ? 'border-[#e8824a] bg-[#e8824a]' : !conn && 'bg-white border-[#9aa0a6] hover:border-[#e8824a]')}
                      style={conn && !dragging ? { background: color, borderColor: '#fff' } : undefined}
                      onPointerDown={e => startConnect(e, n.id, port)}
                      title={t('port_out')}
                    />
                  </div>
                ) })}

              {/* Sous-nœud IA : port de sortie centré sur le BORD SUPÉRIEUR (se branche sur un agent). */}
              {aiOut && (
                <div
                  data-ai-out={`${n.id}|${aiOut}`}
                  onPointerDown={e => startAiFromSub(e, n.id, aiOut)}
                  className="absolute left-1/2 top-0 -translate-x-1/2 -translate-y-1/2 w-3.5 h-3.5 rounded-full border-2 border-[#f1f3f4] bg-[#6750a4] hover:scale-125 cursor-crosshair"
                  style={{ transition: 'transform .1s' }}
                  title={aiOut}
                />
              )}

              {/* Agent IA : ports de SOUS-ENTRÉE répartis sous le nœud. Le losange est
                  centré EXACTEMENT sur (px, py) = point d'ancrage de l'arête IA. */}
              {subInputs.map((si, i) => {
                const px = subPortX(i, subInputs.length, w)
                const py = nodeBoxHeight(n, meta, !!log)
                const filled = edges.some(ed => ed.target === n.id && ed.target_port === si.id)
                return (
                  <div key={si.id} className="absolute" style={{ left: px, top: py }}>
                    <div
                      data-ai-in={`${n.id}|${si.id}|${si.kind}`}
                      onPointerDown={e => startAiFromAgent(e, n.id, si.id, si.kind, n.position.x + px, n.position.y + py)}
                      className={clsx('w-3 h-3 rotate-45 -translate-x-1/2 -translate-y-1/2 border-2 cursor-crosshair shadow-sm',
                        aiDrag?.sub && aiDrag.kind === si.kind ? 'border-[#f1f3f4] bg-[#6750a4] scale-150'
                        : filled ? 'border-white bg-[#6750a4]' : 'bg-white border-[#b3a4d4] hover:border-[#6750a4]')}
                      style={{ transition: 'transform .1s' }}
                    />
                  </div>
                )
              })}
            </div>
          )
        })}
      </div>

      {/* Rectangle de sélection */}
      {mrect && (mrect.w > 1 || mrect.h > 1) && (
        <div
          className="absolute border border-[#e8824a] bg-[#e8824a]/10 pointer-events-none rounded-sm"
          style={{ left: mrect.x, top: mrect.y, width: mrect.w, height: mrect.h }}
        />
      )}

      {connectDrag && (
        <div className="absolute top-2 left-1/2 -translate-x-1/2 bg-[#e8824a] text-white text-xs px-3 py-1 rounded-full shadow pointer-events-none">
          {t('connect_hint')}
        </div>
      )}

      {renderMinimap()}

      {/* Contrôle zoom */}
      <div className="absolute bottom-3 right-3 flex items-center gap-1 bg-[#ffffff] border border-[#dadce0] rounded-lg px-2 py-1 text-[#5f6368] text-xs no-print">
        <button className="px-1.5 hover:text-[#202124]" onClick={() => zoomBy(0.9)}>−</button>
        <span className="w-10 text-center">{Math.round(vp.scale * 100)}%</span>
        <button className="px-1.5 hover:text-[#202124]" onClick={() => zoomBy(1.1)}>+</button>
        <button className="px-1.5 hover:text-[#202124]" onClick={fitToContent} title={t('ctx_fit', { defaultValue: 'Ajuster' })}>⤢</button>
        <button className="px-1.5 hover:text-[#202124]" onClick={resetView} title={t('reset')}>⟲</button>
      </div>

      {menu && (
        <MenuDropdown items={menu.items} pos={{ top: menu.y, left: menu.x }} onClose={() => setMenu(null)} />
      )}
    </div>
  )
}
