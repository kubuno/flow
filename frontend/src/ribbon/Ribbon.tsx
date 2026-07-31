// <Ribbon> — ruban façon MS Office, partagé par tous les sous-éditeurs Office.
// Rendu data-driven depuis `RibbonTab[]` : bande d'onglets (+ onglets contextuels à
// droite avec liseré coloré) puis, pour l'onglet actif, une rangée de GROUPES (boîte
// + libellé en bas, séparés par des filets). Les petits items se rangent en colonnes
// de 3 (comme Office) ; les gros boutons occupent toute la hauteur du groupe.
import { Fragment, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import { useTranslation } from 'react-i18next'
import { ChevronDown, ChevronUp } from 'lucide-react'
import { Dropdown, MenuDropdown, useIsMobile } from '@ui'
import type { MenuItem, MenuDropdownPos } from '@ui'
import type { WorkspaceTheme } from '@kubuno/sdk'
import { fileAccentFor } from './officeThemes'
import type { RibbonTab, RibbonGroup, RibbonItem } from './types'

const TAB_H        = 30   // hauteur de la bande d'onglets
const CONTENT_H    = 84   // hauteur de la zone de groupes (items + libellé)
const MOBILE_BAR_H = 48   // hauteur de la barre de commandes mobile (en bas)

// Bande d'onglets du ruban, extraite pour pouvoir être rendue soit EN LIGNE au-dessus
// des groupes (défaut historique), soit DANS LA TOPBAR du WorkspaceShell (`inTopbar` :
// onglets alignés en bas, fond transparent — la topbar a déjà la couleur de bande).
// Déplacée dans la topbar, la rangée dédiée disparaît → 30px de hauteur gagnés, et le
// mode recherche (overlay opaque sur la topbar) masque les onglets sans rien décaler.
export function RibbonTabStrip({ tabs, theme, activeId, onSelect, tabStripActions, inTopbar = false, height, collapsed = false, onToggleCollapsed }: {
  tabs: RibbonTab[]
  theme: WorkspaceTheme
  activeId?: string
  onSelect: (id: string) => void
  // Bloc d'actions (Enregistrer/Annuler/Rétablir/étoile/corbeille) inséré juste après
  // l'onglet « Fichier » (ou en tête si pas d'onglet Fichier).
  tabStripActions?: ReactNode
  inTopbar?: boolean
  height?: number          // hauteur explicite en mode topbar (= topbarHeight)
  // Bouton de réduction du ruban : rendu À LA FIN de la bande d'onglets (donc TOUJOURS
  // visible, y compris replié — plus de « barre fine » sous les onglets).
  collapsed?: boolean
  onToggleCollapsed?: () => void
}) {
  const { t } = useTranslation()
  const visibleTabs = tabs.filter(t => t.visible !== false)
  const colored = !!theme.topbarText
  const stripBg = colored ? (theme.topbarBg ?? theme.accent) : theme.header
  const tabInactive = colored ? (theme.topbarText ?? '#ffffff') : theme.textDim
  const fileTab = visibleTabs.find(t => t.backstage != null)
  // activeId nullish (ruban replié SANS peek) → aucun onglet actif (façon Office).
  const cur = activeId != null ? visibleTabs.find(t => t.id === activeId) : undefined
  const tabH = colored ? 'h-[26px]' : (inTopbar ? 'h-[30px]' : 'h-full')
  return (
    <div data-ribbon-tabs className={`flex items-end gap-0.5 ${inTopbar ? '' : 'px-2'}`}
         style={inTopbar ? { height: height ?? '100%' } : { height: TAB_H, background: stripBg }}>
      {/* Pas d'onglet « Fichier » → le bloc d'actions ouvre la bande. */}
      {!fileTab && tabStripActions && (
        <div className="flex items-center gap-0.5 self-center mr-1">{tabStripActions}</div>
      )}
      {visibleTabs.map(tab => {
        const isActive = tab.id === cur?.id
        const ctx = tab.contextual
        const isFile = tab === fileTab
        // Onglet Fichier : pastille pleine couleur d'accent (façon Office). Le bloc
        // d'actions (Enregistrer/Annuler/…/corbeille) est inséré JUSTE APRÈS.
        if (isFile) {
          return (
            <Fragment key={tab.id}>
              <button onClick={() => onSelect(tab.id)}
                className={`relative px-3.5 text-[14px] font-semibold ${tabH} rounded-t`}
                style={{ color: 'var(--kbn-office-file-accent-text, #fff)', background: `var(--kbn-office-file-accent, ${fileAccentFor(theme.accent)})`, borderTopLeftRadius: 5, borderTopRightRadius: 5 }}
                onMouseEnter={e => { e.currentTarget.style.filter = 'brightness(1.1)' }}
                onMouseLeave={e => { e.currentTarget.style.filter = 'none' }}>
                {tab.label}
              </button>
              {tabStripActions && (
                <div className="flex items-center gap-0.5 self-center mx-1">{tabStripActions}</div>
              )}
            </Fragment>
          )
        }
        return (
          <button key={tab.id} onClick={() => onSelect(tab.id)}
            className={`relative px-3.5 text-[14px] font-medium ${colored ? 'h-[26px]' : (inTopbar ? 'h-[30px] rounded-t' : 'h-full rounded-t')}`}
            style={{
              color: isActive ? `var(--kbn-office-tab-active-text, ${theme.accent})` : tabInactive,
              background: isActive ? theme.bg : 'transparent',
              borderTopLeftRadius: 5,
              borderTopRightRadius: 5,
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
      {/* Bouton de réduction du ruban, TOUJOURS visible en fin de bande d'onglets.
          Aligné vers le BAS (presque au niveau des onglets) plutôt que centré. */}
      {onToggleCollapsed && (
        <div className="self-end mb-0.5 ml-1 flex-shrink-0">
          <RibbonCollapseButton
            collapsed={collapsed} onToggle={onToggleCollapsed} colored={colored}
            label={collapsed
              ? t('office_ribbon_expand',   { defaultValue: 'Développer le ruban (Ctrl+F1)' })
              : t('office_ribbon_collapse', { defaultValue: 'Réduire le ruban (Ctrl+F1)' })} />
        </div>
      )}
    </div>
  )
}

// Bouton de réduction/expansion du ruban (façon MS Office). Chevron HAUT = réduire
// (le ruban se replie vers le haut) ; chevron BAS = développer. Infobulle obligatoire.
// `colored` : posé sur la bande d'onglets COLORÉE (topbar) → texte clair.
function RibbonCollapseButton({ collapsed = false, onToggle, label, colored = false }: {
  collapsed?: boolean
  onToggle: () => void
  label: string
  colored?: boolean
}) {
  return (
    <button
      type="button"
      onClick={onToggle}
      title={label}
      aria-label={label}
      aria-expanded={!collapsed}
      className={`h-6 w-6 rounded flex items-center justify-center transition-colors ${colored ? 'text-white/70 hover:bg-white/15 hover:text-white' : 'text-text-tertiary hover:bg-black/5 hover:text-text-primary'}`}
    >
      {collapsed ? <ChevronDown size={16} /> : <ChevronUp size={16} />}
    </button>
  )
}

export function Ribbon({ tabs, theme, activeTabId, onTabChange, tabStripActions, hideTabs = false, collapsed = false, peeking = false, onClosePeek }: {
  tabs: RibbonTab[]
  theme: WorkspaceTheme
  activeTabId?: string
  onTabChange?: (id: string) => void
  // Bloc d'actions (Enregistrer/Annuler/Rétablir/étoile/corbeille) inséré DANS la
  // bande d'onglets, juste après l'onglet « Fichier » (desktop uniquement).
  tabStripActions?: ReactNode
  // La bande d'onglets est rendue AILLEURS (dans la topbar, via RibbonTabStrip) :
  // le ruban ne rend que la rangée de groupes (+ backstage).
  hideTabs?: boolean
  // Réduction du ruban (état piloté par OfficeShell, qui possède aussi les onglets) :
  // `collapsed` = replié (rangée de groupes masquée, ruban = 0px) ; `peeking` = un onglet
  // est temporairement affiché en flyout flottant alors que le ruban est replié. Le BOUTON
  // de réduction n'est PAS ici : il vit dans la bande d'onglets (RibbonTabStrip, topbar).
  collapsed?: boolean
  peeking?: boolean
  onClosePeek?: () => void
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

  // ── Onglet « Fichier » (Backstage façon Office) ─────────────────────────────────
  // Repéré par `backstage` non vide. Quand actif, on rend ce contenu en OVERLAY plein
  // module (mesuré sur la racine du ruban → couvre ruban + zone d'édition, garde
  // l'en-tête du WorkspaceShell au-dessus). Stylé avec l'accent de l'app.
  const backstageActive = cur?.backstage != null
  const isMobile = useIsMobile()

  // ── Réduction du ruban (façon MS Office) ────────────────────────────────────────
  // L'état (replié / peek) est piloté par OfficeShell ; le BOUTON vit dans la bande
  // d'onglets (topbar). Ici : rendu + fermeture du flyout de peek quand le focus sort.
  // Peek (ruban replié + onglet cliqué) : le flyout se ferme dès qu'une action se produit
  // HORS du ruban et HORS de la bande d'onglets (clic dans le document, Échap).
  const peekRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (!peeking) return
    const onDown = (e: MouseEvent) => {
      const node = e.target as Node
      if (peekRef.current?.contains(node)) return
      if ((node as Element).closest?.('[data-ribbon-tabs]')) return
      onClosePeek?.()
    }
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') onClosePeek?.() }
    document.addEventListener('mousedown', onDown, true)
    document.addEventListener('keydown', onKey)
    return () => { document.removeEventListener('mousedown', onDown, true); document.removeEventListener('keydown', onKey) }
  }, [peeking, onClosePeek])

  const rootRef = useRef<HTMLDivElement>(null)
  const [bsBox, setBsBox] = useState<{ top: number; left: number; width: number; height: number } | null>(null)
  useLayoutEffect(() => {
    if (!backstageActive) { setBsBox(null); return }
    // Le backstage couvre la zone SOUS la bande d'onglets (qui reste visible, façon
    // Office) jusqu'au bas du module ; largeur = ruban (pas le viewport → sinon
    // déborde sur le rail droit). Synchro par rAF : suit EN CONTINU la position du
    // ruban (repli/dépli du panneau latéral animé, resize…) ; ne re-render que si la
    // boîte change réellement (sinon React bail-out). Sur mobile la bande d'onglets
    // n'existe pas (barre du bas) : le backstage part du haut du ruban. Idem quand la
    // bande est rendue ailleurs (topbar, `hideTabs`).
    const topOffset = (isMobile || hideTabs) ? 0 : TAB_H
    let raf = 0
    const tick = () => {
      const r = rootRef.current?.getBoundingClientRect()
      if (r) {
        const next = { top: r.top + topOffset, left: r.left, width: r.width, height: window.innerHeight - (r.top + topOffset) }
        setBsBox(prev => (prev && prev.top === next.top && prev.left === next.left && prev.width === next.width && prev.height === next.height) ? prev : next)
      }
      raf = requestAnimationFrame(tick)
    }
    raf = requestAnimationFrame(tick)
    return () => cancelAnimationFrame(raf)
  }, [backstageActive, isMobile, hideTabs])

  // ── Version MOBILE : barre de commandes en BAS + palette bottom-sheet ─────────
  // (le ruban desktop — bande d'onglets + rangée de groupes — n'est pas rendu).
  // `tabs` vide (ex. mode LECTURE des documents) → aucune barre : plein écran.
  if (isMobile) {
    return (
      <div ref={rootRef} style={{ flexShrink: 0, userSelect: 'none' }}>
        {visibleTabs.length > 0 && <MobileRibbon tabs={visibleTabs} cur={cur} setActive={setActive} theme={theme} />}
        {backstageActive && bsBox != null && createPortal(
          <div style={{ position: 'fixed', top: bsBox.top, left: bsBox.left, width: bsBox.width, height: bsBox.height, zIndex: 40, background: theme.bg, overflow: 'auto' }}>
            {cur?.backstage}
          </div>,
          document.body,
        )}
      </div>
    )
  }

  return (
    <div ref={rootRef} style={{ flexShrink: 0, userSelect: 'none' }}>
      {/* Bande d'onglets (masquée si rendue ailleurs — dans la topbar) */}
      {!hideTabs && (
        <RibbonTabStrip tabs={visibleTabs} theme={theme} activeId={cur?.id} onSelect={setActive} tabStripActions={tabStripActions} />
      )}

      {/* Rangée de groupes de l'onglet actif (vide pour l'onglet Fichier : son
          contenu est rendu en overlay sous la bande d'onglets, qui reste visible).
          Repli responsive : quand la rangée déborde, les groupes de DROITE se replient
          un à un en gros boutons déroulants (cf. RibbonGroupsRow). */}
      {collapsed ? (
        // Replié + un onglet cliqué → PEEK : la rangée de groupes s'affiche en FLYOUT
        // FLOTTANT (par-dessus le contenu, sans le pousser), et se referme au 1er clic
        // hors du ruban / des onglets. Replié SANS peek → RIEN (ruban = 0px, pas de barre).
        peeking ? (
          <div className="relative" style={{ height: 0 }}>
            <div ref={peekRef} className="absolute left-0 right-0 top-0 z-40"
                 style={{ boxShadow: '0 10px 26px rgba(0,0,0,0.20)' }}>
              <RibbonGroupsRow key={cur?.id} groups={cur?.groups ?? []} theme={theme} height={CONTENT_H} />
            </div>
          </div>
        ) : null
      ) : (
        <RibbonGroupsRow key={cur?.id} groups={cur?.groups ?? []} theme={theme} height={CONTENT_H} />
      )}

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
      <div className="text-[10px] text-center whitespace-nowrap" style={{ color: theme.textDim }}>{group.label}</div>
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
      <div className="text-[10px] text-center whitespace-nowrap" style={{ color: theme.textDim }}>{group.label}</div>
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
      onClick={() => { if ((item.kind === 'split' || item.kind === 'menu') && !item.onClick) openSplit(); else item.onClick?.() }}
      className={`flex ${large ? 'flex-col w-14 h-full items-center justify-center gap-1' : 'flex-row items-center h-[20px] px-1.5 gap-1'} rounded-xs disabled:opacity-40`}
      style={{ background: activeBg, color: activeFg }}
      onMouseEnter={e => { if (!item.active) e.currentTarget.style.background = theme.dark ? 'rgba(255,255,255,0.08)' : 'var(--kbn-ws-hover, #f1f3f4)' }}
      onMouseLeave={e => { if (!item.active) e.currentTarget.style.background = 'transparent' }}>
      <span className="flex items-center justify-center" style={{ width: large ? 22 : 16, height: large ? 22 : 16 }}>{item.icon}</span>
      {(large || item.label) && <span className={large ? 'text-[10px] leading-tight text-center' : 'text-[11px] whitespace-nowrap'}>{item.label}</span>}
      {/* Chevron SOUS le libellé (gros bouton à menu, façon Word). */}
      {item.kind === 'menu' && <ChevronDown size={11} style={{ color: theme.textDim, marginTop: large ? -2 : 0 }} />}
    </button>
  )

  // Bouton dont TOUTE la surface ouvre un menu : contenu libre (`menuRender`)
  // sinon liste d'actions (`splitItems`).
  if (item.kind === 'menu') {
    return (
      <span className="flex items-center">
        {core}
        {menu && (
          <MenuDropdown
            items={item.menuRender
              ? [{ type: 'custom', render: close => item.menuRender!(close) }]
              : (item.splitItems ?? []).map<MenuItem>(si => ({
                  type: 'action', label: si.label ?? si.id, icon: si.icon, checked: si.active, disabled: si.disabled,
                  onClick: () => { if (!si.disabled) si.onClick?.() },
                }))}
            pos={{ ...menu, minWidth: item.menuRender ? undefined : 180 }} onClose={() => setMenu(null)} />
        )}
      </span>
    )
  }

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
            items={item.menuRender ? [{ type: 'custom', render: close => item.menuRender!(close) }] : (item.splitItems ?? []).map<MenuItem>(si => ({
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

// ── Ruban MOBILE ──────────────────────────────────────────────────────────────
// Remplace le ruban desktop sous 1024px (façon Google Docs / Office mobile) :
//  · une BARRE fixe en bas de l'écran — pilule « onglet actif ⌃ » + les commandes
//    de l'onglet à plat (icônes, défilement horizontal, filets entre groupes) ;
//  · la pilule ouvre une PALETTE bottom-sheet mi-hauteur : sélecteur d'onglets en
//    tête (y compris « Fichier »/backstage et les onglets contextuels) puis les
//    groupes de l'onglet actif empilés verticalement, items avec libellés.
// Entièrement généré des mêmes RibbonTab[] : aucune commande n'est perdue.
function MobileRibbon({ tabs, cur, setActive, theme }: {
  tabs: RibbonTab[]
  cur: RibbonTab | undefined
  setActive: (id: string) => void
  theme: WorkspaceTheme
}) {
  const [paletteOpen, setPaletteOpen] = useState(false)

  // La barre suit le CLAVIER VIRTUEL (façon Word mobile) : visualViewport donne
  // la hauteur réellement visible → la barre se colle au-dessus du clavier
  // quand il est ouvert, au bas de l'écran sinon. Les petits écarts (<60px,
  // barre d'URL qui se replie…) sont ignorés.
  const [kbInset, setKbInset] = useState(0)
  useEffect(() => {
    const vv = window.visualViewport
    if (!vv) return
    const update = () => {
      const inset = Math.max(0, Math.round(window.innerHeight - vv.height - vv.offsetTop))
      setKbInset(inset < 60 ? 0 : inset)
    }
    vv.addEventListener('resize', update)
    vv.addEventListener('scroll', update)
    update()
    return () => { vv.removeEventListener('resize', update); vv.removeEventListener('scroll', update) }
  }, [])

  // Barre du bas ÉPURÉE : `mobileQuick` du groupe s'il est déclaré (sélection
  // manifeste de l'éditeur, ex. B/I/U du groupe Police custom des documents ;
  // `[]` = groupe absent de la barre), sinon sélection automatique = boutons
  // d'action à icône (button/toggle/split) — les dropdowns, galeries et
  // contrôles riches satureraient la rangée ; ils vivent dans la palette.
  const flat: RibbonItem[] = []
  for (const g of cur?.groups ?? []) {
    const keep = g.mobileQuick ?? g.items.filter(it =>
      (it.kind === 'button' || it.kind === 'toggle' || it.kind === 'split' || it.kind === 'menu') && it.icon)
    if (!keep.length) continue
    if (flat.length) flat.push({ id: `__gsep_${g.id}`, kind: 'separator' as const })
    flat.push(...keep)
  }

  const tint = theme.dark ? 'rgba(255,255,255,0.10)'
    : (/^#[0-9a-fA-F]{6}$/.test(theme.accent) ? `${theme.accent}1f` : `color-mix(in srgb, ${theme.accent} 12%, transparent)`)

  return createPortal(
    <>
      {/* Barre de commandes : bas de l'écran, ou juste AU-DESSUS du clavier. */}
      <div data-app-chrome className="fixed left-0 right-0 z-[45] flex items-center gap-1 pl-1.5 pr-1"
        style={{ bottom: kbInset,
          height: kbInset ? MOBILE_BAR_H : `calc(${MOBILE_BAR_H}px + env(safe-area-inset-bottom))`,
          paddingBottom: kbInset ? 0 : 'env(safe-area-inset-bottom)',
          background: theme.bg, borderTop: `1px solid ${theme.border}` }}>
        <button onClick={() => setPaletteOpen(o => !o)}
          className="flex items-center gap-1 h-9 px-3 rounded-full text-xs font-medium flex-shrink-0"
          style={{ color: `var(--kbn-office-tab-active-text, ${theme.accent})`, background: tint }}>
          {cur?.label}
          <ChevronUp size={14} style={{ transform: paletteOpen ? 'rotate(180deg)' : undefined, transition: 'transform .15s' }} />
        </button>
        <div className="flex items-center gap-0.5 flex-1 overflow-x-auto overflow-y-hidden self-stretch px-1"
          style={{ scrollbarWidth: 'none' }}>
          {flat.map(it => <MobileItemView key={it.id} item={it} theme={theme} />)}
        </div>
      </div>

      {/* Palette : le ruban entier en feuille mi-hauteur. */}
      {paletteOpen && (
        <>
          <div className="fixed inset-0 z-[46] bg-black/40" style={{ animation: 'kb-sheet-fade .15s ease-out' }}
            onClick={() => setPaletteOpen(false)} />
          <div className="fixed left-0 right-0 z-[47] flex flex-col rounded-t-2xl overflow-hidden"
            style={{ bottom: kbInset, maxHeight: '62vh', background: theme.bg, boxShadow: '0 -8px 32px rgba(0,0,0,0.25)',
              paddingBottom: kbInset ? 0 : 'env(safe-area-inset-bottom)', animation: 'kb-sheet-up .18s ease-out' }}>
            {/* Sélecteur d'onglets (Fichier stylé accent, contextuels avec liseré). */}
            <div className="flex items-center gap-1 px-2 pt-2.5 pb-2 overflow-x-auto flex-shrink-0"
              style={{ borderBottom: `1px solid ${theme.border}`, scrollbarWidth: 'none' }}>
              {tabs.map(t => {
                const activeT = t.id === cur?.id
                if (t.backstage != null) {
                  return (
                    <button key={t.id} onClick={() => { setPaletteOpen(false); setActive(t.id) }}
                      className="h-8 px-3.5 rounded-full text-xs font-semibold flex-shrink-0"
                      style={{ color: 'var(--kbn-office-file-accent-text, #fff)', background: `var(--kbn-office-file-accent, ${fileAccentFor(theme.accent)})` }}>
                      {t.label}
                    </button>
                  )
                }
                return (
                  <button key={t.id} onClick={() => setActive(t.id)}
                    className="h-8 px-3.5 rounded-full text-xs font-medium flex-shrink-0"
                    style={{ color: activeT ? `var(--kbn-office-tab-active-text, ${theme.accent})` : theme.textDim,
                      background: activeT ? tint : 'transparent',
                      boxShadow: t.contextual ? `inset 0 2px 0 ${t.contextual.accent}` : undefined }}>
                    {t.label}
                  </button>
                )
              })}
            </div>
            {/* Groupes de l'onglet actif, empilés — chaque commande est une TUILE
                LIBELLÉE (grille 2 colonnes, texte = label ou tooltip) ; les
                contrôles riches (dropdown/galerie/custom) en rangées pleine
                largeur, dans l'ordre d'origine du groupe. */}
            <div className="flex-1 overflow-y-auto px-3 py-2.5">
              {(cur?.groups ?? []).map(g => (
                <div key={g.id} className="mb-3">
                  <div className="text-[11px] font-semibold uppercase tracking-wide mb-1" style={{ color: theme.textDim }}>
                    {g.label}
                  </div>
                  <PaletteGroup items={g.items} theme={theme} onAction={() => setPaletteOpen(false)} />
                </div>
              ))}
            </div>
          </div>
        </>
      )}
    </>,
    document.body,
  )
}

// ── Palette : équilibre lisibilité / densité verticale ────────────────────────
// · Les TOGGLES à icône (gras, alignements, listes… — état visuel, icônes
//   connues) forment des rangées d'icônes COMPACTES (cluster horizontal, façon
//   Office mobile) → 6-8 commandes par rangée de 36px.
// · Les ACTIONS (button/split, et toggles sans icône) restent des tuiles
//   LIBELLÉES (texte = label ?? tooltip) en grille 2 colonnes — c'est là que le
//   libellé apporte du sens.
// · Dropdown / galerie / custom : rangées pleine largeur, ordre du groupe
//   préservé. Les séparateurs sont inutiles (chaque groupe a son en-tête).
function PaletteGroup({ items, theme, onAction }: { items: RibbonItem[]; theme: WorkspaceTheme; onAction?: () => void }) {
  type Block = { kind: 'tiles' | 'toggles'; items: RibbonItem[] } | { kind: 'wide'; item: RibbonItem }
  const blocks: Block[] = []
  let run: RibbonItem[] = []
  let runKind: 'tiles' | 'toggles' = 'tiles'
  const flush = () => { if (run.length) { blocks.push({ kind: runKind, items: run }); run = [] } }
  const push = (kind: 'tiles' | 'toggles', it: RibbonItem) => {
    if (run.length && runKind !== kind) flush()
    runKind = kind
    run.push(it)
  }
  for (const it of items) {
    if (it.kind === 'separator') { flush(); continue }
    if (it.kind === 'toggle' && it.icon && !it.paletteTile) push('toggles', it)
    else if (it.kind === 'button' || it.kind === 'split' || it.kind === 'menu' || it.kind === 'toggle') push('tiles', it)
    else { flush(); blocks.push({ kind: 'wide', item: it }) }
  }
  flush()
  return (
    <div className="flex flex-col gap-1.5">
      {blocks.map((b, i) => b.kind === 'wide' ? (
        <PaletteWideRow key={i} item={b.item} theme={theme} />
      ) : b.kind === 'toggles' ? (
        <div key={i} className="flex flex-wrap gap-1">
          {b.items.map(it => <PaletteToggle key={it.id} item={it} theme={theme} />)}
        </div>
      ) : (
        <div key={i} className="grid grid-cols-2 gap-1">
          {b.items.map(it => <PaletteTile key={it.id} item={it} theme={theme} onAction={onAction} />)}
        </div>
      ))}
    </div>
  )
}

// Bascule compacte : tuile d'icône 44×36 (le title porte le libellé) — l'état
// actif teinté rend la fonction évidente, comme sur le ruban desktop.
function PaletteToggle({ item, theme }: { item: RibbonItem; theme: WorkspaceTheme }) {
  const tint = /^#[0-9a-fA-F]{6}$/.test(theme.accent)
    ? `${theme.accent}22`
    : `color-mix(in srgb, ${theme.accent} 13%, transparent)`
  const activeBg = item.active ? (theme.dark ? 'rgba(255,255,255,0.14)' : `var(--kbn-office-item-active-bg, ${tint})`) : (theme.dark ? 'rgba(255,255,255,0.05)' : 'rgba(0,0,0,0.03)')
  const activeFg = item.active ? `var(--kbn-office-item-active-text, ${theme.accent})` : theme.text
  return (
    <button title={item.tooltip ?? item.label} disabled={item.disabled} onClick={() => item.onClick?.()}
      className="flex items-center justify-center rounded-lg disabled:opacity-40"
      style={{ width: 44, height: 36, background: activeBg, color: activeFg }}>
      <span className="flex items-center justify-center" style={{ width: 18, height: 18 }}>{item.icon}</span>
    </button>
  )
}

function PaletteTile({ item, theme, onAction }: { item: RibbonItem; theme: WorkspaceTheme; onAction?: () => void }) {
  const [menu, setMenu] = useState<MenuDropdownPos | null>(null)
  const btnRef = useRef<HTMLButtonElement>(null)
  const text = item.label ?? item.tooltip ?? item.id

  const tint = /^#[0-9a-fA-F]{6}$/.test(theme.accent)
    ? `${theme.accent}22`
    : `color-mix(in srgb, ${theme.accent} 13%, transparent)`
  const activeBg = item.active ? (theme.dark ? 'rgba(255,255,255,0.14)' : `var(--kbn-office-item-active-bg, ${tint})`) : (theme.dark ? 'rgba(255,255,255,0.05)' : 'rgba(0,0,0,0.03)')
  const activeFg = item.active ? `var(--kbn-office-item-active-text, ${theme.accent})` : theme.text

  const openSplit = () => {
    const r = btnRef.current?.getBoundingClientRect()
    if (r) setMenu({ top: r.top - 4, left: r.left })
  }

  return (
    <span className="flex items-stretch min-w-0 rounded-lg overflow-hidden" style={{ background: activeBg }}>
      <button ref={btnRef} disabled={item.disabled}
        onClick={() => {
          if ((item.kind === 'split' || item.kind === 'menu') && !item.onClick) { openSplit(); return }
          item.onClick?.()
          // Une commande exécutée referme la palette (façon Office mobile) : son
          // effet est sur la diapo/le document, cachés derrière la feuille. Les
          // BASCULES de mise en forme (gras…) restent, elles s'enchaînent —
          // sauf celles marquées `paletteTile` (choix de VUE ou d'OUTIL : on veut
          // s'en servir tout de suite sur le document).
          if (item.kind !== 'toggle' || item.paletteTile) onAction?.()
        }}
        className="flex items-center gap-2 h-9 px-2 text-left flex-1 min-w-0 disabled:opacity-40"
        style={{ color: activeFg }}>
        <span className="flex items-center justify-center flex-shrink-0" style={{ width: 18, height: 18 }}>{item.icon}</span>
        <span className="text-xs truncate">{text}</span>
      </button>
      {item.kind === 'split' && (
        <button onClick={openSplit} disabled={item.disabled}
          className="flex items-center justify-center w-7 flex-shrink-0 disabled:opacity-40"
          style={{ color: theme.textDim, borderLeft: `1px solid ${theme.border}` }}>
          <ChevronDown size={13} />
        </button>
      )}
      {menu && (
        <MenuDropdown
          items={item.menuRender ? [{ type: 'custom', render: close => item.menuRender!(close) }] : (item.splitItems ?? []).map<MenuItem>(si => ({
            type: 'action', label: si.label ?? si.id, checked: si.active, disabled: si.disabled,
            onClick: () => { if (!si.disabled) si.onClick?.() },
          }))}
          pos={{ ...menu, minWidth: 180 }} onClose={() => setMenu(null)} />
      )}
    </span>
  )
}

// Rangée pleine largeur de la palette : dropdown (libellé à gauche si connu),
// galerie (options défilables), custom (rendu tel quel).
function PaletteWideRow({ item, theme }: { item: RibbonItem; theme: WorkspaceTheme }) {
  const text = item.label ?? item.tooltip
  if (item.kind === 'dropdown') {
    return (
      <div className="flex items-center gap-2.5 min-h-10">
        {text && <span className="text-xs flex-shrink-0 w-24 truncate" style={{ color: theme.textDim }}>{text}</span>}
        <Dropdown value={item.value ?? ''} onChange={v => item.onChange?.(v)}
          options={item.options ?? []} width={item.width ?? 150} />
      </div>
    )
  }
  if (item.kind === 'gallery') {
    return (
      <div className="flex items-center gap-1 overflow-x-auto py-0.5" style={{ scrollbarWidth: 'none' }}>
        {(item.options ?? []).map(o => (
          <button key={o.value} title={o.label} onClick={() => item.onChange?.(o.value)}
            className="px-2.5 h-10 rounded-lg text-xs flex-shrink-0"
            style={{ color: theme.text, border: `1px solid ${theme.border}` }}>
            {o.icon ?? o.label}
          </button>
        ))}
      </div>
    )
  }
  // custom
  return <div className="flex items-center flex-wrap gap-1.5 min-h-10">{item.render}</div>
}

// Item du ruban en rendu TACTILE : cible ≥ 36px, icône 18px. `labeled` (palette)
// affiche aussi le libellé ; la barre du bas reste icône seule (tooltip = title).
function MobileItemView({ item, theme, labeled }: { item: RibbonItem; theme: WorkspaceTheme; labeled?: boolean }) {
  const [menu, setMenu] = useState<MenuDropdownPos | null>(null)
  const btnRef = useRef<HTMLButtonElement>(null)
  const tip = [item.tooltip ?? item.label, item.shortcut].filter(Boolean).join(' · ')

  if (item.kind === 'separator') {
    return <div className="self-stretch flex-shrink-0" style={{ width: 1, background: theme.border, margin: labeled ? '2px 4px' : '8px 4px' }} />
  }
  if (item.kind === 'custom') return <span className="flex items-center flex-shrink-0">{item.render}</span>
  if (item.kind === 'dropdown') {
    return (
      <span className="flex-shrink-0">
        <Dropdown value={item.value ?? ''} onChange={v => item.onChange?.(v)}
          options={item.options ?? []} width={item.width ?? 110} />
      </span>
    )
  }
  if (item.kind === 'gallery') {
    return (
      <span className="flex items-center gap-0.5 flex-shrink-0">
        {(item.options ?? []).map(o => (
          <button key={o.value} title={o.label} onClick={() => item.onChange?.(o.value)}
            className="px-2 h-9 rounded text-xs" style={{ color: theme.text, border: `1px solid ${theme.border}` }}>
            {o.icon ?? o.label}
          </button>
        ))}
      </span>
    )
  }

  const tint = /^#[0-9a-fA-F]{6}$/.test(theme.accent)
    ? `${theme.accent}22`
    : `color-mix(in srgb, ${theme.accent} 13%, transparent)`
  const activeBg = item.active ? (theme.dark ? 'rgba(255,255,255,0.14)' : `var(--kbn-office-item-active-bg, ${tint})`) : 'transparent'
  const activeFg = item.active ? `var(--kbn-office-item-active-text, ${theme.accent})` : theme.text

  const openSplit = () => {
    const r = btnRef.current?.getBoundingClientRect()
    if (r) setMenu({ top: r.top - 4, left: r.left })
  }

  return (
    <span className="flex items-center flex-shrink-0">
      <button ref={btnRef} title={tip} disabled={item.disabled}
        onClick={() => { if ((item.kind === 'split' || item.kind === 'menu') && !item.onClick) openSplit(); else item.onClick?.() }}
        className={`flex items-center justify-center gap-1.5 rounded-lg disabled:opacity-40 ${labeled ? 'h-9 px-2.5 min-w-9' : 'h-10 min-w-10 px-1.5'}`}
        style={{ background: activeBg, color: activeFg }}>
        {item.icon && <span className="flex items-center justify-center" style={{ width: 18, height: 18 }}>{item.icon}</span>}
        {(labeled || !item.icon) && item.label && <span className="text-xs whitespace-nowrap">{item.label}</span>}
      </button>
      {item.kind === 'split' && (
        <button title={tip} onClick={openSplit}
          className="flex items-center justify-center w-5 self-stretch" style={{ color: theme.textDim }}>
          <ChevronDown size={12} />
        </button>
      )}
      {menu && (
        <MenuDropdown
          items={item.menuRender ? [{ type: 'custom', render: close => item.menuRender!(close) }] : (item.splitItems ?? []).map<MenuItem>(si => ({
            type: 'action', label: si.label ?? si.id, checked: si.active, disabled: si.disabled,
            onClick: () => { if (!si.disabled) si.onClick?.() },
          }))}
          pos={{ ...menu, minWidth: 180 }} onClose={() => setMenu(null)} />
      )}
    </span>
  )
}
