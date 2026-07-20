// <Ribbon> — ruban façon MS Office, partagé par tous les sous-éditeurs Office.
// Rendu data-driven depuis `RibbonTab[]` : bande d'onglets (+ onglets contextuels à
// droite avec liseré coloré) puis, pour l'onglet actif, une rangée de GROUPES (boîte
// + libellé en bas, séparés par des filets). Les petits items se rangent en colonnes
// de 3 (comme Office) ; les gros boutons occupent toute la hauteur du groupe.
import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { ChevronDown } from 'lucide-react'
import { Dropdown, MenuDropdown } from '@ui'
import type { MenuItem, MenuDropdownPos } from '@ui'
import type { WorkspaceTheme } from '@kubuno/sdk'
import { fileAccentFor } from './officeThemes'
import type { RibbonTab, RibbonGroup, RibbonItem } from './types'

const TAB_H     = 30   // hauteur de la bande d'onglets
const CONTENT_H = 84   // hauteur de la zone de groupes (items + libellé)

export function Ribbon({ tabs, theme, activeTabId, onTabChange }: {
  tabs: RibbonTab[]
  theme: WorkspaceTheme
  activeTabId?: string
  onTabChange?: (id: string) => void
}) {
  const visibleTabs = tabs.filter(t => t.visible !== false)
  const [internalActive, setInternalActive] = useState<string>(visibleTabs[0]?.id ?? '')
  const active = activeTabId ?? internalActive
  const setActive = (id: string) => { onTabChange?.(id); if (activeTabId === undefined) setInternalActive(id) }

  // Auto-bascule sur un onglet contextuel qui VIENT d'apparaître (sélection d'objet),
  // et repli sur le 1er onglet si l'onglet actif disparaît.
  const prevCtxRef = useRef<string[]>([])
  useEffect(() => {
    const ctxNow = visibleTabs.filter(t => t.contextual).map(t => t.id)
    const fresh = ctxNow.find(id => !prevCtxRef.current.includes(id))
    prevCtxRef.current = ctxNow
    if (fresh) { setActive(fresh); return }
    // Repli si l'onglet actif disparaît : on choisit le 1ᵉʳ onglet NORMAL (jamais
    // l'onglet « Fichier »/backstage, sinon désélectionner un objet ouvrirait le
    // backstage de façon intempestive).
    if (!visibleTabs.some(t => t.id === active)) {
      const firstNormal = visibleTabs.find(t => t.backstage == null) ?? visibleTabs[0]
      if (firstNormal) setActive(firstNormal.id)
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [visibleTabs.map(t => t.id).join('|')])

  const cur = visibleTabs.find(t => t.id === active) ?? visibleTabs[0]

  // Mode « ruban coloré » (façon Word) : bande d'onglets BLEUE (= topbarBg) qui se fond
  // dans la topbar, onglet actif = carte BLANCHE à coins arrondis se raccordant au contenu.
  const colored = !!theme.topbarText
  const stripBg = colored ? (theme.topbarBg ?? theme.accent) : theme.header
  const tabInactive = colored ? (theme.topbarText ?? '#ffffff') : theme.textDim

  // ── Onglet « Fichier » (Backstage façon Office) ─────────────────────────────────
  // Repéré par `backstage` non vide. Quand actif, on rend ce contenu en OVERLAY plein
  // module (mesuré sur la racine du ruban → couvre ruban + zone d'édition, garde
  // l'en-tête du WorkspaceShell au-dessus). Stylé avec l'accent de l'app.
  const fileTab = visibleTabs.find(t => t.backstage != null)
  const backstageActive = cur?.backstage != null
  const rootRef = useRef<HTMLDivElement>(null)
  const [bsBox, setBsBox] = useState<{ top: number; left: number; width: number; height: number } | null>(null)
  useLayoutEffect(() => {
    if (!backstageActive) { setBsBox(null); return }
    // Le backstage couvre la zone SOUS la bande d'onglets (qui reste visible, façon
    // Office) jusqu'au bas du module ; largeur = ruban (pas le viewport → sinon
    // déborde sur le rail droit). Synchro par rAF : suit EN CONTINU la position du
    // ruban (repli/dépli du panneau latéral animé, resize…) ; ne re-render que si la
    // boîte change réellement (sinon React bail-out).
    let raf = 0
    const tick = () => {
      const r = rootRef.current?.getBoundingClientRect()
      if (r) {
        const next = { top: r.top + TAB_H, left: r.left, width: r.width, height: window.innerHeight - (r.top + TAB_H) }
        setBsBox(prev => (prev && prev.top === next.top && prev.left === next.left && prev.width === next.width && prev.height === next.height) ? prev : next)
      }
      raf = requestAnimationFrame(tick)
    }
    raf = requestAnimationFrame(tick)
    return () => cancelAnimationFrame(raf)
  }, [backstageActive])

  return (
    <div ref={rootRef} style={{ flexShrink: 0, userSelect: 'none' }}>
      {/* Bande d'onglets */}
      <div className="flex items-end px-2 gap-0.5" style={{ height: TAB_H, background: stripBg }}>
        {visibleTabs.map(tab => {
          const isActive = tab.id === cur?.id
          const ctx = tab.contextual
          const isFile = tab === fileTab
          // Onglet Fichier : pastille pleine couleur d'accent (façon Office).
          if (isFile) {
            return (
              <button key={tab.id} onClick={() => setActive(tab.id)}
                className={`relative px-3.5 text-[12px] font-semibold ${colored ? 'h-[26px]' : 'h-full'} rounded-t`}
                style={{ color: 'var(--kbn-office-file-accent-text, #fff)', background: `var(--kbn-office-file-accent, ${fileAccentFor(theme.accent)})`, borderTopLeftRadius: colored ? 8 : 4, borderTopRightRadius: colored ? 8 : 4 }}
                onMouseEnter={e => { e.currentTarget.style.filter = 'brightness(1.1)' }}
                onMouseLeave={e => { e.currentTarget.style.filter = 'none' }}>
                {tab.label}
              </button>
            )
          }
          return (
            <button key={tab.id} onClick={() => setActive(tab.id)}
              className={`relative px-3.5 text-[12px] font-medium ${colored ? 'h-[26px]' : 'h-full rounded-t'}`}
              style={{
                color: isActive ? `var(--kbn-office-tab-active-text, ${theme.accent})` : tabInactive,
                background: isActive ? theme.bg : 'transparent',
                borderTopLeftRadius: colored ? 8 : undefined,
                borderTopRightRadius: colored ? 8 : undefined,
                borderTop: ctx ? `2px solid ${ctx.accent}` : undefined,
              }}
              onMouseEnter={e => { if (!isActive) e.currentTarget.style.background = colored ? 'var(--kbn-office-strip-hover, rgba(255,255,255,0.16))' : (theme.dark ? 'rgba(255,255,255,0.08)' : 'var(--kbn-ws-hover, #f1f3f4)') }}
              onMouseLeave={e => { if (!isActive) e.currentTarget.style.background = 'transparent' }}>
              {ctx && <span style={{ color: isActive ? ctx.accent : tabInactive, marginRight: 4, fontSize: 9 }}>●</span>}
              {tab.label}
              {!colored && isActive && <span style={{ position: 'absolute', left: 6, right: 6, bottom: 0, height: 2, background: theme.accent, borderRadius: 2 }} />}
            </button>
          )
        })}
      </div>

      {/* Rangée de groupes de l'onglet actif (vide pour l'onglet Fichier : son
          contenu est rendu en overlay sous la bande d'onglets, qui reste visible).
          Repli responsive : quand la rangée déborde, les groupes de DROITE se replient
          un à un en gros boutons déroulants (cf. RibbonGroupsRow). */}
      <RibbonGroupsRow key={cur?.id} groups={cur?.groups ?? []} theme={theme} height={CONTENT_H} />

      {/* Backstage (onglet Fichier actif) : overlay SOUS la bande d'onglets. */}
      {backstageActive && bsBox != null && createPortal(
        <div style={{ position: 'fixed', top: bsBox.top, left: bsBox.left, width: bsBox.width, height: bsBox.height, zIndex: 40, background: theme.bg, overflow: 'hidden' }}>
          {cur?.backstage}
        </div>,
        document.body,
      )}
    </div>
  )
}

// Rangée de groupes AVEC repli responsive (façon MS Office). Tant que tout tient, les
// groupes sont rendus normalement. Dès que la rangée déborde en largeur, on replie les
// groupes EN PARTANT DE LA DROITE, un par un, en « gros boutons déroulants » (chips) :
// le contenu du groupe est alors accessible dans un popover. On mesure la largeur
// NATURELLE de chaque groupe (offsetWidth quand déployé) et la largeur du chip (quand
// replié), mémorisées par id, puis on calcule le nb minimal de groupes à replier.
function RibbonGroupsRow({ groups, theme, height }: { groups: RibbonGroup[]; theme: WorkspaceTheme; height: number }) {
  const rowRef = useRef<HTMLDivElement>(null)
  const naturalRef = useRef<Map<string, number>>(new Map())   // id → largeur déployée
  const collapsedRef = useRef<Map<string, number>>(new Map()) // id → largeur repliée (chip)
  const expandedEls = useRef<Map<string, HTMLDivElement>>(new Map())
  const chipEls = useRef<Map<string, HTMLDivElement>>(new Map())
  const [collapsed, setCollapsed] = useState(0)
  const [containerW, setContainerW] = useState(0)

  // Largeur disponible de la rangée (suit resize + repli/dépli du panneau latéral).
  useLayoutEffect(() => {
    const el = rowRef.current
    if (!el) return
    const measure = () => setContainerW(el.clientWidth)
    const ro = new ResizeObserver(measure)
    ro.observe(el)
    measure()
    return () => ro.disconnect()
  }, [])

  const n = groups.length
  // Estimation de repli avant la 1ʳᵉ mesure réelle du chip (icône + libellé + chevron).
  const chipEstimate = (g: RibbonGroup) => 40 + g.label.length * 6.5

  // Après chaque rendu : (1) mémorise les largeurs mesurées, (2) recalcule le nb de
  // groupes à replier. Sans tableau de deps → converge (setCollapsed n'est appelé qu'au
  // changement ; les largeurs mémorisées sont stables une fois mesurées).
  useLayoutEffect(() => {
    for (let i = 0; i < n; i++) {
      const g = groups[i]
      if (i < n - collapsed) {
        const el = expandedEls.current.get(g.id)
        if (el && el.offsetWidth > 0) naturalRef.current.set(g.id, el.offsetWidth)
      } else {
        const el = chipEls.current.get(g.id)
        if (el && el.offsetWidth > 0) collapsedRef.current.set(g.id, el.offsetWidth)
      }
    }
    if (containerW <= 0 || n === 0) return
    const natW = (g: RibbonGroup) => naturalRef.current.get(g.id) ?? 120
    const colW = (g: RibbonGroup) => collapsedRef.current.get(g.id) ?? chipEstimate(g)
    // Largeur totale si l'on replie les `c` groupes les plus à droite.
    const totalFor = (c: number) => {
      let s = 0
      for (let i = 0; i < n; i++) s += i < n - c ? natW(groups[i]) : colW(groups[i])
      return s
    }
    let c = 0
    while (c < n && totalFor(c) > containerW - 1) c++
    if (c !== collapsed) setCollapsed(c)
  })

  return (
    // overflow-hidden : plus de barre de défilement — on replie au lieu de défiler.
    <div ref={rowRef} className="flex items-stretch px-2 overflow-hidden" style={{ height, background: theme.bg, borderBottom: `1px solid ${theme.border}` }}>
      {groups.map((g, i) => {
        const last = i === n - 1
        const setEl = (map: { current: Map<string, HTMLDivElement> }) => (el: HTMLDivElement | null) => { if (el) map.current.set(g.id, el); else map.current.delete(g.id) }
        if (i < n - collapsed) {
          return (
            <div key={g.id} ref={setEl(expandedEls)} className="flex flex-shrink-0">
              <RibbonGroupView group={g} theme={theme} last={last} />
            </div>
          )
        }
        return (
          <div key={g.id} ref={setEl(chipEls)} className="flex flex-shrink-0">
            <CollapsedGroupView group={g} theme={theme} last={last} />
          </div>
        )
      })}
    </div>
  )
}

// Groupe REPLIÉ : gros bouton (icône représentative + libellé + chevron) occupant la
// place d'un groupe ; un clic ouvre un popover contenant le groupe entier (rendu normal).
function CollapsedGroupView({ group, theme, last }: { group: RibbonGroup; theme: WorkspaceTheme; last: boolean }) {
  const [open, setOpen] = useState(false)
  // `anchor` = point d'ancrage brut (sous le bouton) ; `box` = position CLAMPÉE réelle
  // du popover après mesure, pour qu'il reste ENTIÈREMENT dans le viewport.
  const [anchor, setAnchor] = useState<{ top: number; left: number; btnTop: number } | null>(null)
  const [box, setBox] = useState<{ top: number; left: number } | null>(null)
  const btnRef = useRef<HTMLButtonElement>(null)
  const popRef = useRef<HTMLDivElement>(null)
  const icon = group.items.find(it => it.icon)?.icon

  const toggle = () => {
    const r = btnRef.current?.getBoundingClientRect()
    if (r) setAnchor({ top: r.bottom + 2, left: r.left, btnTop: r.top })
    setBox(null)
    setOpen(o => !o)
  }
  // Après ouverture : mesure le popover et le repositionne pour qu'il tienne dans la
  // fenêtre (débord droite → recalé vers la gauche ; débord bas → basculé au-dessus).
  useLayoutEffect(() => {
    if (!open || !anchor || !popRef.current) return
    const el = popRef.current
    const w = el.offsetWidth, h = el.offsetHeight, m = 8
    let left = Math.min(anchor.left, window.innerWidth - m - w)
    left = Math.max(m, left)
    let top = anchor.top
    if (top + h > window.innerHeight - m) {
      const above = anchor.btnTop - 2 - h
      top = above >= m ? above : Math.max(m, window.innerHeight - m - h)
    }
    setBox(prev => (prev && Math.abs(prev.left - left) < 0.5 && Math.abs(prev.top - top) < 0.5) ? prev : { top, left })
  }, [open, anchor])
  useEffect(() => {
    if (!open) return
    const onDown = (e: MouseEvent) => {
      if (popRef.current?.contains(e.target as Node) || btnRef.current?.contains(e.target as Node)) return
      setOpen(false)
    }
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') setOpen(false) }
    document.addEventListener('mousedown', onDown, true)
    document.addEventListener('keydown', onKey)
    return () => { document.removeEventListener('mousedown', onDown, true); document.removeEventListener('keydown', onKey) }
  }, [open])

  return (
    <div className="flex flex-col items-center justify-between flex-shrink-0 px-2 py-0.5"
      style={{ borderRight: last ? undefined : `1px solid ${theme.border}` }}>
      <button ref={btnRef} title={group.label} onMouseDown={e => e.preventDefault()} onClick={toggle}
        className="flex flex-col items-center justify-center gap-1 flex-1 rounded-xs px-2"
        style={{ minWidth: 48, background: open ? (theme.dark ? 'rgba(255,255,255,0.10)' : 'var(--kbn-ws-hover, #f1f3f4)') : 'transparent', color: theme.text }}
        onMouseEnter={e => { if (!open) e.currentTarget.style.background = theme.dark ? 'rgba(255,255,255,0.08)' : 'var(--kbn-ws-hover, #f1f3f4)' }}
        onMouseLeave={e => { if (!open) e.currentTarget.style.background = 'transparent' }}>
        {icon && <span className="flex items-center justify-center" style={{ width: 22, height: 22 }}>{icon}</span>}
        <ChevronDown size={12} style={{ color: theme.textDim }} />
      </button>
      <div className="text-[9px] text-center whitespace-nowrap" style={{ color: theme.textDim }}>{group.label}</div>
      {open && anchor && createPortal(
        // Tant que la position clampée n'est pas calculée, on rend le popover invisible
        // (mais mesurable) pour éviter un saut visible. maxWidth borne les très gros
        // groupes sur écran étroit (défilement interne plutôt que débord).
        <div ref={popRef} className="flex items-stretch"
          style={{ position: 'fixed', top: (box ?? anchor).top, left: (box ?? anchor).left, zIndex: 50,
            visibility: box ? 'visible' : 'hidden',
            maxWidth: 'calc(100vw - 16px)', overflowX: 'auto',
            background: theme.bg, border: `1px solid ${theme.border}`, borderRadius: 8, boxShadow: '0 8px 24px rgba(0,0,0,0.18)', padding: 4 }}>
          <RibbonGroupView group={group} theme={theme} last />
        </div>,
        document.body,
      )}
    </div>
  )
}

function RibbonGroupView({ group, theme, last }: { group: RibbonGroup; theme: WorkspaceTheme; last: boolean }) {
  return (
    <div className="flex flex-col items-center justify-between flex-shrink-0 px-2 py-0.5"
      style={{ borderRight: last ? undefined : `1px solid ${theme.border}` }}>
      <div className="flex items-stretch gap-0.5 flex-1">
        {toColumns(group.items).map((col, ci) => (
          <div key={ci} className="flex flex-col justify-center gap-[1px]">
            {col.map(it => <RibbonItemView key={it.id} item={it} theme={theme} />)}
          </div>
        ))}
      </div>
      <div className="text-[9px] text-center whitespace-nowrap" style={{ color: theme.textDim }}>{group.label}</div>
    </div>
  )
}

// RÈGLE : un petit bouton ne peut JAMAIS être sur plus de 3 lignes — on empile les
// petits items consécutifs en colonnes de 3 MAX (au-delà, nouvelle colonne), façon
// Office. Tout le reste (gros bouton, dropdown, gallery, custom, séparateur) forme sa
// propre colonne. Le budget vertical (CONTENT_H 84 − padding − libellé) impose des
// petits boutons de 20px et des interlignes d'1px pour caser les 3 lignes.
const MAX_STACK = 3
function toColumns(items: RibbonItem[]): RibbonItem[][] {
  const cols: RibbonItem[][] = []
  let run: RibbonItem[] = []
  const flush = () => { if (run.length) { cols.push(run); run = [] } }
  for (const it of items) {
    const small = (it.kind === 'button' || it.kind === 'toggle') && (it.size ?? 'small') === 'small'
    if (small) { run.push(it); if (run.length === MAX_STACK) flush() }
    else { flush(); cols.push([it]) }
  }
  flush()
  return cols
}

function RibbonItemView({ item, theme }: { item: RibbonItem; theme: WorkspaceTheme }) {
  const [menu, setMenu] = useState<MenuDropdownPos | null>(null)
  const btnRef = useRef<HTMLButtonElement>(null)
  const tip = [item.tooltip ?? item.label, item.shortcut].filter(Boolean).join(' · ')

  if (item.kind === 'separator') return <div style={{ width: 1, alignSelf: 'stretch', background: theme.border, margin: '0 2px' }} />
  if (item.kind === 'custom') return <>{item.render}</>

  if (item.kind === 'dropdown') {
    return <Dropdown value={item.value ?? ''} onChange={v => item.onChange?.(v)}
      options={item.options ?? []} width={item.width ?? 120} />
  }

  if (item.kind === 'gallery') {
    return (
      <div className="flex items-center gap-0.5">
        {(item.options ?? []).map(o => (
          <button key={o.value} title={o.label} onMouseDown={e => e.preventDefault()} onClick={() => item.onChange?.(o.value)}
            className="px-2 h-7 rounded-xs text-[11px] hover:bg-black/5"
            style={{ color: theme.text, border: `1px solid ${theme.border}` }}>
            {o.icon ?? o.label}
          </button>
        ))}
      </div>
    )
  }

  const large = item.size === 'large'
  // Surbrillance d'item ACTIF teintée par l'accent de l'app (≈12% d'opacité) ; en
  // thème sombre, voile blanc translucide. `color-mix` gère un accent non-hex
  // (chaîne `var(...)` posée par un thème) là où la concat hex+alpha échouerait.
  const tint = /^#[0-9a-fA-F]{6}$/.test(theme.accent)
    ? `${theme.accent}22`
    : `color-mix(in srgb, ${theme.accent} 13%, transparent)`
  const activeBg = item.active ? (theme.dark ? 'rgba(255,255,255,0.14)' : `var(--kbn-office-item-active-bg, ${tint})`) : 'transparent'
  const activeFg = item.active ? `var(--kbn-office-item-active-text, ${theme.accent})` : theme.text

  const openSplit = () => {
    const r = btnRef.current?.getBoundingClientRect()
    if (r) setMenu({ top: r.bottom + 2, left: r.left })
  }

  const core = (
    <button ref={btnRef} title={tip} disabled={item.disabled}
      onMouseDown={e => e.preventDefault()}
      onClick={() => { if (item.kind === 'split' && !item.onClick) openSplit(); else item.onClick?.() }}
      className={`flex ${large ? 'flex-col w-14 h-full items-center justify-center gap-1' : 'flex-row items-center h-[20px] px-1.5 gap-1'} rounded-xs disabled:opacity-40`}
      style={{ background: activeBg, color: activeFg }}
      onMouseEnter={e => { if (!item.active) e.currentTarget.style.background = theme.dark ? 'rgba(255,255,255,0.08)' : 'var(--kbn-ws-hover, #f1f3f4)' }}
      onMouseLeave={e => { if (!item.active) e.currentTarget.style.background = 'transparent' }}>
      <span className="flex items-center justify-center" style={{ width: large ? 22 : 16, height: large ? 22 : 16 }}>{item.icon}</span>
      {(large || item.label) && <span className={large ? 'text-[10px] leading-tight text-center' : 'text-[11px] whitespace-nowrap'}>{item.label}</span>}
    </button>
  )

  if (item.kind === 'split') {
    return (
      <span className="flex items-center">
        {core}
        <button title={tip} onMouseDown={e => e.preventDefault()} onClick={openSplit}
          className="flex items-center justify-center w-4 h-[20px] rounded-xs hover:bg-black/5" style={{ color: theme.textDim }}>
          <ChevronDown size={11} />
        </button>
        {menu && (
          <MenuDropdown
            items={(item.splitItems ?? []).map<MenuItem>(si => ({
              type: 'action', label: si.label ?? si.id, checked: si.active, disabled: si.disabled,
              onClick: () => { if (!si.disabled) si.onClick?.() },
            }))}
            pos={{ ...menu, minWidth: 180 }} onClose={() => setMenu(null)} />
        )}
      </span>
    )
  }

  return core
}
