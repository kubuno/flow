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
  const firstCtx = visibleTabs.find(t => t.contextual)

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
                style={{ color: '#fff', background: fileAccentFor(theme.accent), borderTopLeftRadius: colored ? 8 : 4, borderTopRightRadius: colored ? 8 : 4 }}
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
                color: isActive ? theme.accent : tabInactive,
                background: isActive ? theme.bg : 'transparent',
                borderTopLeftRadius: colored ? 8 : undefined,
                borderTopRightRadius: colored ? 8 : undefined,
                borderTop: ctx ? `2px solid ${ctx.accent}` : undefined,
                marginLeft: ctx && tab === firstCtx ? 'auto' : undefined,
              }}
              onMouseEnter={e => { if (!isActive) e.currentTarget.style.background = colored ? 'rgba(255,255,255,0.16)' : (theme.dark ? 'rgba(255,255,255,0.08)' : '#f1f3f4') }}
              onMouseLeave={e => { if (!isActive) e.currentTarget.style.background = 'transparent' }}>
              {ctx && <span style={{ color: isActive ? ctx.accent : tabInactive, marginRight: 4, fontSize: 9 }}>●</span>}
              {tab.label}
              {!colored && isActive && <span style={{ position: 'absolute', left: 6, right: 6, bottom: 0, height: 2, background: theme.accent, borderRadius: 2 }} />}
            </button>
          )
        })}
      </div>

      {/* Rangée de groupes de l'onglet actif (vide pour l'onglet Fichier : son
          contenu est rendu en overlay sous la bande d'onglets, qui reste visible). */}
      <div className="flex items-stretch px-2 overflow-x-auto" style={{ height: CONTENT_H, background: theme.bg, borderBottom: `1px solid ${theme.border}` }}>
        {cur?.groups.map((g, i) => <RibbonGroupView key={g.id} group={g} theme={theme} last={i === cur.groups.length - 1} />)}
      </div>

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

function RibbonGroupView({ group, theme, last }: { group: RibbonGroup; theme: WorkspaceTheme; last: boolean }) {
  return (
    <div className="flex flex-col items-center justify-between flex-shrink-0 px-2 py-1"
      style={{ borderRight: last ? undefined : `1px solid ${theme.border}` }}>
      <div className="flex items-stretch gap-0.5 flex-1">
        {toColumns(group.items).map((col, ci) => (
          <div key={ci} className="flex flex-col justify-center gap-0.5">
            {col.map(it => <RibbonItemView key={it.id} item={it} theme={theme} />)}
          </div>
        ))}
      </div>
      <div className="text-[10px] mt-0.5 text-center whitespace-nowrap" style={{ color: theme.textDim }}>{group.label}</div>
    </div>
  )
}

// RÈGLE : un petit bouton ne peut JAMAIS être sur plus de 2 lignes — on empile les
// petits items consécutifs en colonnes de 2 MAX (au-delà, nouvelle colonne). Tout le
// reste (gros bouton, dropdown, gallery, custom, séparateur) forme sa propre colonne.
const MAX_STACK = 2
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
            className="px-2 h-7 rounded text-[11px] hover:bg-black/5"
            style={{ color: theme.text, border: `1px solid ${theme.border}` }}>
            {o.icon ?? o.label}
          </button>
        ))}
      </div>
    )
  }

  const large = item.size === 'large'
  // Surbrillance d'item ACTIF teintée par l'accent de l'app (≈12% d'opacité) ; en
  // thème sombre, voile blanc translucide.
  const tint = /^#[0-9a-fA-F]{6}$/.test(theme.accent) ? `${theme.accent}22` : '#e8f0fe'
  const activeBg = item.active ? (theme.dark ? 'rgba(255,255,255,0.14)' : tint) : 'transparent'
  const activeFg = item.active ? theme.accent : theme.text

  const openSplit = () => {
    const r = btnRef.current?.getBoundingClientRect()
    if (r) setMenu({ top: r.bottom + 2, left: r.left })
  }

  const core = (
    <button ref={btnRef} title={tip} disabled={item.disabled}
      onMouseDown={e => e.preventDefault()}
      onClick={() => { if (item.kind === 'split' && !item.onClick) openSplit(); else item.onClick?.() }}
      className={`flex ${large ? 'flex-col w-14 h-full justify-center gap-1' : 'flex-row items-center h-[22px] px-1.5 gap-1'} rounded disabled:opacity-40`}
      style={{ background: activeBg, color: activeFg }}
      onMouseEnter={e => { if (!item.active) e.currentTarget.style.background = theme.dark ? 'rgba(255,255,255,0.08)' : '#f1f3f4' }}
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
          className="flex items-center justify-center w-4 h-[22px] rounded hover:bg-black/5" style={{ color: theme.textDim }}>
          <ChevronDown size={11} />
        </button>
        {menu && (
          <MenuDropdown
            items={(item.splitItems ?? []).map<MenuItem>(si => ({
              type: 'action', label: si.label ?? si.id, checked: si.active, onClick: () => si.onClick?.(),
            }))}
            pos={{ ...menu, minWidth: 180 }} onClose={() => setMenu(null)} />
        )}
      </span>
    )
  }

  return core
}
