import { useCallback, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import * as Icons from 'lucide-react'
import clsx from 'clsx'
import { MenuDropdown, type MenuItem } from '@ui'
import type { NodeLog, NodeMeta, WorkflowEdge, WorkflowNode } from './types'

export const NODE_W = 210
const HEADER_Y = 30 // centre vertical de l'en-tête (port d'entrée + sortie par défaut)
const PORT_BASE = 54
const PORT_GAP = 22

function LucideIcon({ name, size = 16, color }: { name: string; size?: number; color?: string }) {
  const Cmp = (Icons as unknown as Record<string, React.ComponentType<{ size?: number; color?: string }>>)[name] ?? Icons.Box
  return <Cmp size={size} color={color} />
}

function outputs(meta: NodeMeta | undefined): string[] {
  if (!meta) return ['default']
  return meta.outputs.length > 0 ? meta.outputs.map(o => o.id) : ['default']
}

function outPortY(meta: NodeMeta | undefined, portId: string): number {
  const outs = outputs(meta)
  if (outs.length <= 1) return HEADER_Y
  const idx = Math.max(0, outs.indexOf(portId))
  return PORT_BASE + idx * PORT_GAP
}

interface Viewport { tx: number; ty: number; scale: number }

interface Props {
  nodes: WorkflowNode[]
  edges: WorkflowEdge[]
  metas: Map<string, NodeMeta>
  selectedIds: Set<string>
  logs: Map<string, NodeLog>
  onSelectionChange: (ids: Set<string>) => void
  onMoveNode: (id: string, x: number, y: number) => void
  onConnect: (source: string, sourcePort: string, target: string) => void
  onDeleteEdge: (id: string) => void
  onSetWaypoints: (edgeId: string, waypoints: { x: number; y: number }[]) => void
  onDeleteNode: (id: string) => void
  onDeleteSelected: () => void
  onDuplicateNode: (id: string) => void
  onRenameNode: (id: string) => void
  onCopyNode: (id: string) => void
  onDisconnectNode: (id: string) => void
  onPaste: () => void
  canPaste: boolean
  onRequestAddNode: () => void
}

const NODE_H = 92 // hauteur approximative d'un nœud (pour le test d'intersection marquee)

export default function FlowCanvas({
  nodes, edges, metas, selectedIds, logs, onSelectionChange, onMoveNode, onConnect, onDeleteEdge, onSetWaypoints,
  onDeleteNode, onDeleteSelected, onDuplicateNode, onRenameNode, onCopyNode, onDisconnectNode, onPaste, canPaste, onRequestAddNode,
}: Props) {
  const { t } = useTranslation('flow')
  const containerRef = useRef<HTMLDivElement>(null)
  const [vp, setVp] = useState<Viewport>({ tx: 40, ty: 40, scale: 1 })
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null)
  const [connecting, setConnecting] = useState<{ source: string; port: string } | null>(null)
  // Glisser de nœud(s) : déplace toute la sélection en bloc.
  const drag = useRef<{ startX: number; startY: number; items: { id: string; origX: number; origY: number }[] } | null>(null)
  const pan = useRef<{ x: number; y: number; tx: number; ty: number } | null>(null)
  // Sélection au rectangle (clic gauche sur le fond) — coords relatives au conteneur.
  const marquee = useRef<{ sx: number; sy: number; base: Set<string> } | null>(null)
  const [mrect, setMrect] = useState<{ x: number; y: number; w: number; h: number } | null>(null)
  const [grabbing, setGrabbing] = useState(false)
  // Glisser d'un point de passage d'une connexion.
  const wpDrag = useRef<{ edgeId: string; index: number } | null>(null)

  const byId = useCallback((id: string) => nodes.find(n => n.id === id), [nodes])

  // Convertit des coordonnées écran en coordonnées « monde » du plan de travail.
  const toWorld = useCallback((clientX: number, clientY: number) => {
    const r = containerRef.current!.getBoundingClientRect()
    return { x: (clientX - r.left - vp.tx) / vp.scale, y: (clientY - r.top - vp.ty) / vp.scale }
  }, [vp])

  // Nœuds intersectant un rectangle (coords écran relatives au conteneur).
  const nodesInRect = useCallback((r: { x: number; y: number; w: number; h: number }): string[] => {
    const ids: string[] = []
    for (const n of nodes) {
      const nx = vp.tx + n.position.x * vp.scale
      const ny = vp.ty + n.position.y * vp.scale
      const nw = NODE_W * vp.scale, nh = NODE_H * vp.scale
      if (nx < r.x + r.w && nx + nw > r.x && ny < r.y + r.h && ny + nh > r.y) ids.push(n.id)
    }
    return ids
  }, [nodes, vp])

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
  const fitToContent = () => {
    if (!nodes.length || !containerRef.current) { resetView(); return }
    const minX = Math.min(...nodes.map(n => n.position.x)), minY = Math.min(...nodes.map(n => n.position.y))
    const maxX = Math.max(...nodes.map(n => n.position.x + NODE_W)), maxY = Math.max(...nodes.map(n => n.position.y + 90))
    const r = containerRef.current.getBoundingClientRect(), pad = 60
    const scale = Math.min(2.5, Math.max(0.25, Math.min((r.width - pad * 2) / (maxX - minX || 1), (r.height - pad * 2) / (maxY - minY || 1))))
    setVp({ tx: pad - minX * scale, ty: pad - minY * scale, scale })
  }
  const multi = selectedIds.size > 1
  const nodeMenu = (n: WorkflowNode): MenuItem[] => [
    { type: 'action', label: t('ctx_configure',  { defaultValue: 'Configurer' }),         icon: ic('Settings2'),      onClick: () => onSelectOne(n.id) },
    { type: 'action', label: t('ctx_rename',      { defaultValue: 'Renommer' }),           icon: ic('Pencil'),         onClick: () => onRenameNode(n.id) },
    { type: 'action', label: t('ctx_duplicate',   { defaultValue: 'Dupliquer' }),          shortcut: 'Ctrl+D', icon: ic('CopyPlus'), onClick: () => onDuplicateNode(n.id) },
    { type: 'action', label: t('ctx_copy',        { defaultValue: 'Copier' }),             shortcut: 'Ctrl+C', icon: ic('Copy'),     onClick: () => onCopyNode(n.id) },
    { type: 'action', label: t('ctx_disconnect',  { defaultValue: 'Détacher les liens' }), icon: ic('Unlink'),         onClick: () => onDisconnectNode(n.id) },
    { type: 'separator' },
    (multi && selectedIds.has(n.id)
      ? { type: 'action', label: t('ctx_delete_selection', { defaultValue: 'Supprimer la sélection ({{count}})', count: selectedIds.size }), shortcut: 'Suppr', icon: ic('Trash2'), onClick: onDeleteSelected }
      : { type: 'action', label: t('ctx_delete',           { defaultValue: 'Supprimer le nœud' }),                                          shortcut: 'Suppr', icon: ic('Trash2'), onClick: () => onDeleteNode(n.id) }),
  ]
  const edgeMenu = (e: WorkflowEdge, world: { x: number; y: number }): MenuItem[] => [
    { type: 'action', label: t('ctx_add_point',    { defaultValue: 'Ajouter un point' }),        icon: ic('Spline'),   onClick: () => addWaypointAt(e, world) },
    { type: 'separator' },
    { type: 'action', label: t('ctx_delete_edge',  { defaultValue: 'Supprimer la connexion' }),  icon: ic('Scissors'), onClick: () => onDeleteEdge(e.id) },
  ]
  const canvasMenu = (): MenuItem[] => [
    { type: 'action', label: t('ctx_add_node',   { defaultValue: 'Ajouter un nœud' }), icon: ic('Plus'),            onClick: onRequestAddNode },
    { type: 'action', label: t('ctx_paste',      { defaultValue: 'Coller' }),          shortcut: 'Ctrl+V', disabled: !canPaste, icon: ic('ClipboardPaste'), onClick: onPaste },
    { type: 'separator' },
    { type: 'action', label: t('ctx_zoom_in',    { defaultValue: 'Zoom avant' }),      icon: ic('ZoomIn'),          onClick: () => zoomBy(1.2) },
    { type: 'action', label: t('ctx_zoom_out',   { defaultValue: 'Zoom arrière' }),    icon: ic('ZoomOut'),         onClick: () => zoomBy(0.8) },
    { type: 'action', label: t('ctx_fit',        { defaultValue: 'Ajuster à l’écran' }), icon: ic('Maximize'), onClick: fitToContent },
    { type: 'action', label: t('ctx_reset_view', { defaultValue: 'Réinitialiser la vue' }),   icon: ic('RotateCcw'), onClick: resetView },
  ]

  // ── Pan & zoom ────────────────────────────────────────────────
  const onWheel = (e: React.WheelEvent) => {
    e.preventDefault()
    const factor = e.deltaY > 0 ? 0.9 : 1.1
    setVp(v => ({ ...v, scale: Math.min(2.5, Math.max(0.25, v.scale * factor)) }))
  }
  const onSelectOne = (id: string) => onSelectionChange(new Set([id]))

  // Clic CENTRAL (bouton 1) = déplacer le plan de travail (n'importe où).
  // Clic GAUCHE (bouton 0) sur le fond = sélection au rectangle (marquee).
  const onContainerPointerDown = (e: React.PointerEvent) => {
    if (e.button === 1) {
      e.preventDefault()
      setConnecting(null)
      setGrabbing(true)
      pan.current = { x: e.clientX, y: e.clientY, tx: vp.tx, ty: vp.ty }
      containerRef.current?.setPointerCapture?.(e.pointerId)
      return
    }
    if (e.button !== 0) return
    if (e.target !== containerRef.current && !(e.target as HTMLElement).dataset.bg) return
    setConnecting(null)
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
    if (e.button !== 0) return // bouton central → remonte au conteneur (pan)
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
    if (wpDrag.current) {
      const w = toWorld(e.clientX, e.clientY)
      const edge = edges.find(x => x.id === wpDrag.current!.edgeId)
      if (edge) {
        const wps = [...(edge.waypoints ?? [])]
        wps[wpDrag.current.index] = { x: Math.round(w.x), y: Math.round(w.y) }
        onSetWaypoints(edge.id, wps)
      }
    } else if (drag.current) {
      const dx = (e.clientX - drag.current.startX) / vp.scale
      const dy = (e.clientY - drag.current.startY) / vp.scale
      for (const it of drag.current.items) onMoveNode(it.id, it.origX + dx, it.origY + dy)
    } else if (pan.current) {
      // Capturer la valeur du ref AVANT setVp : l'updater s'exécute plus tard et
      // `pan.current` peut déjà être null (pointerup) → « reading 'tx' of null ».
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
  const onPointerUp = () => { drag.current = null; pan.current = null; marquee.current = null; wpDrag.current = null; setMrect(null); setGrabbing(false) }

  // ── Connexion (clic sortie → clic entrée) ─────────────────────
  const clickOutput = (e: React.MouseEvent, source: string, port: string) => {
    e.stopPropagation()
    setConnecting({ source, port })
  }
  const clickInput = (e: React.MouseEvent, target: string) => {
    e.stopPropagation()
    if (connecting && connecting.source !== target) {
      onConnect(connecting.source, connecting.port, target)
    }
    setConnecting(null)
  }

  // ── Rendu des arêtes ──────────────────────────────────────────
  type Pt = { x: number; y: number }

  // Points de l'arête : sortie source → points de passage → entrée cible (coords monde).
  function edgePoints(e: WorkflowEdge): { pts: Pt[]; src: Pt; dst: Pt } | null {
    const s = byId(e.source); const t = byId(e.target)
    if (!s || !t) return null
    const sm = metas.get(s.type)
    const src = { x: s.position.x + NODE_W, y: s.position.y + outPortY(sm, e.source_port ?? 'default') }
    const dst = { x: t.position.x, y: t.position.y + HEADER_Y }
    return { pts: [src, ...(e.waypoints ?? []), dst], src, dst }
  }

  function edgePath(e: WorkflowEdge): string | null {
    const ep = edgePoints(e)
    if (!ep) return null
    const { pts, src, dst } = ep
    // Sans point de passage : courbe de Bézier horizontale (look « flux » gauche→droite).
    if (pts.length === 2) {
      const dx = Math.max(40, Math.abs(dst.x - src.x) / 2)
      return `M ${src.x} ${src.y} C ${src.x + dx} ${src.y}, ${dst.x - dx} ${dst.y}, ${dst.x} ${dst.y}`
    }
    // Avec points de passage : spline lisse (Catmull-Rom → Bézier) à travers tous les points.
    let d = `M ${pts[0].x} ${pts[0].y}`
    for (let i = 0; i < pts.length - 1; i++) {
      const p0 = pts[i - 1] ?? pts[i]
      const p1 = pts[i]
      const p2 = pts[i + 1]
      const p3 = pts[i + 2] ?? p2
      const c1x = p1.x + (p2.x - p0.x) / 6, c1y = p1.y + (p2.y - p0.y) / 6
      const c2x = p2.x - (p3.x - p1.x) / 6, c2y = p2.y - (p3.y - p1.y) / 6
      d += ` C ${c1x} ${c1y}, ${c2x} ${c2y}, ${p2.x} ${p2.y}`
    }
    return d
  }

  // Distance d'un point à un segment (pour insérer un waypoint au bon endroit).
  function distToSeg(p: Pt, a: Pt, b: Pt): number {
    const dx = b.x - a.x, dy = b.y - a.y
    const len2 = dx * dx + dy * dy || 1
    let tt = ((p.x - a.x) * dx + (p.y - a.y) * dy) / len2
    tt = Math.max(0, Math.min(1, tt))
    const px = a.x + tt * dx, py = a.y + tt * dy
    return Math.hypot(p.x - px, p.y - py)
  }

  // Ajoute un point de passage à une arête, inséré sur le segment le plus proche.
  const addWaypointAt = (e: WorkflowEdge, world: Pt) => {
    const ep = edgePoints(e); if (!ep) return
    const { pts } = ep
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
      onContextMenu={e => openMenu(e, canvasMenu())}
    >
      <div style={{ position: 'absolute', transform: `translate(${vp.tx}px, ${vp.ty}px) scale(${vp.scale})`, transformOrigin: '0 0' }}>
        {/* Arêtes */}
        <svg style={{ position: 'absolute', overflow: 'visible', pointerEvents: 'none', left: 0, top: 0 }}>
          {edges.map(e => {
            const d = edgePath(e)
            if (!d) return null
            const log = logs.get(e.target)
            const color = log?.status === 'success' ? '#1e8e3e' : log?.status === 'error' ? '#d93025' : '#5f6368'
            return (
              <g key={e.id}>
                <path d={d} stroke={color} strokeWidth={2} fill="none" />
                {/* Zone de clic épaisse : double-clic = ajoute un point, clic droit = menu. */}
                <path
                  d={d} stroke="transparent" strokeWidth={16} fill="none"
                  style={{ pointerEvents: 'stroke', cursor: 'copy' }}
                  onDoubleClick={ev => { ev.stopPropagation(); addWaypointAt(e, toWorld(ev.clientX, ev.clientY)) }}
                  onContextMenu={ev => openMenu(ev, edgeMenu(e, toWorld(ev.clientX, ev.clientY)))}
                />
                {/* Poignées des points de passage : glisser pour déplacer, clic droit / double-clic = retirer. */}
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
        </svg>

        {/* Nœuds */}
        {nodes.map(n => {
          const meta = metas.get(n.type)
          const outs = outputs(meta)
          const log = logs.get(n.id)
          const ring = log?.status === 'success' ? 'ring-2 ring-green-500' : log?.status === 'error' ? 'ring-2 ring-red-500' : ''
          const hasInput = (meta?.inputs ?? 1) > 0
          return (
            <div
              key={n.id}
              className={clsx('absolute rounded-lg border border-[#dadce0] bg-[#ffffff] shadow-lg select-none', selectedIds.has(n.id) && 'ring-2 ring-[#e8824a]', ring)}
              style={{ left: n.position.x, top: n.position.y, width: NODE_W }}
              onPointerDown={e => onNodePointerDown(e, n)}
              onContextMenu={e => { if (!selectedIds.has(n.id)) onSelectOne(n.id); openMenu(e, nodeMenu(n)) }}
            >
              <div className="flex items-center gap-2 px-3 py-2 rounded-t-lg" style={{ background: meta?.color ?? '#80868b' }}>
                <LucideIcon name={meta?.icon ?? 'Box'} size={16} color="#fff" />
                <span className="text-white text-sm font-medium truncate">{n.name || meta?.name || n.type}</span>
              </div>
              <div className="px-3 py-2 text-[11px] text-[#80868b] truncate">{meta?.name ?? n.type}</div>

              {/* Port d'entrée */}
              {hasInput && (
                <div
                  className="absolute -left-2 w-3.5 h-3.5 rounded-full border-2 border-[#f1f3f4] bg-[#80868b] hover:bg-[#e8824a] cursor-crosshair"
                  style={{ top: HEADER_Y - 7 }}
                  onClick={e => clickInput(e, n.id)}
                  title={t('port_in')}
                />
              )}
              {/* Ports de sortie */}
              {outs.map((port, i) => (
                <div key={port} className="absolute -right-2 flex items-center" style={{ top: (outs.length <= 1 ? HEADER_Y : PORT_BASE + i * PORT_GAP) - 7 }}>
                  {outs.length > 1 && <span className="absolute right-4 text-[9px] text-[#80868b] whitespace-nowrap">{meta?.outputs[i]?.label}</span>}
                  <div
                    className={clsx('w-3.5 h-3.5 rounded-full border-2 border-[#f1f3f4] cursor-crosshair', connecting?.source === n.id && connecting?.port === port ? 'bg-[#e8824a]' : 'bg-[#80868b] hover:bg-[#e8824a]')}
                    onClick={e => clickOutput(e, n.id, port)}
                    title={t('port_out')}
                  />
                </div>
              ))}
            </div>
          )
        })}
      </div>

      {/* Rectangle de sélection (clic gauche sur le fond) */}
      {mrect && (mrect.w > 1 || mrect.h > 1) && (
        <div
          className="absolute border border-[#e8824a] bg-[#e8824a]/10 pointer-events-none rounded-sm"
          style={{ left: mrect.x, top: mrect.y, width: mrect.w, height: mrect.h }}
        />
      )}

      {connecting && (
        <div className="absolute top-2 left-1/2 -translate-x-1/2 bg-[#e8824a] text-white text-xs px-3 py-1 rounded-full shadow">
          {t('connect_hint')}
        </div>
      )}

      {/* Contrôle zoom */}
      <div className="absolute bottom-3 right-3 flex items-center gap-1 bg-[#ffffff] border border-[#dadce0] rounded-lg px-2 py-1 text-[#5f6368] text-xs">
        <button className="px-1.5 hover:text-[#202124]" onClick={() => setVp(v => ({ ...v, scale: Math.max(0.25, v.scale * 0.9) }))}>−</button>
        <span className="w-10 text-center">{Math.round(vp.scale * 100)}%</span>
        <button className="px-1.5 hover:text-[#202124]" onClick={() => setVp(v => ({ ...v, scale: Math.min(2.5, v.scale * 1.1) }))}>+</button>
        <button className="px-1.5 hover:text-[#202124]" onClick={() => setVp({ tx: 40, ty: 40, scale: 1 })} title={t('reset')}>⟲</button>
      </div>

      {menu && (
        <MenuDropdown items={menu.items} pos={{ top: menu.y, left: menu.x }} onClose={() => setMenu(null)} />
      )}
    </div>
  )
}
