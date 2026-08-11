// OfficeShell — la chrome COMMUNE à tous les sous-éditeurs Office, qui remplace la
// barre de menus + la toolbar par un RUBAN (façon MS Office). Enveloppe le
// `WorkspaceShell` du core : reprend toutes ses props (topbar, titre, retour, thème,
// actions, statut, rail, bottom/status bar, corps…) et rend le ruban dans le slot
// `menuBar` du shell (donc `menuBar`/`menus`/`optionsBar` ne sont PAS exposés ici :
// le ruban les remplace). Chaque sous-éditeur fournit sa config `ribbon: RibbonTab[]`.
import { useCallback, useEffect, useMemo, useState, type ComponentProps } from 'react'
import { useLocation } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { Trash2, Maximize2, Minimize2 } from 'lucide-react'
import { WorkspaceShell, WORKSPACE_OFFICE, useSidebarStore, useConfirm } from '@kubuno/sdk'
import { useIsMobile, ConfirmDialog } from '@ui'
import { Ribbon, RibbonTabStrip } from '../ribbon/Ribbon'
import type { RibbonTab, RibbonGroup, RibbonItem } from '../ribbon/types'

// Réduction du ruban (façon MS Office) — PRÉFIXE de clé localStorage : la clé réelle est
// suffixée par le sous-module (cf. `ribbonKey`) → persistance PAR sous-module. Défaut = déplié.
const RIBBON_COLLAPSED_KEY = 'kubuno.ribbon.collapsed'

// Garantit, pour TOUT ruban, un onglet « Affichage » (id `view`) contenant un groupe
// « Afficher » (id `show`) avec un bouton « Plein écran » (id `fullscreen`) qui bascule
// le plein écran (Fullscreen API, façon F11). Si l'onglet/groupe existent déjà, on ne
// fait qu'y AJOUTER le bouton (sans doublon) ; sinon on les crée. Ruban vide (mode
// lecture) → inchangé.
function ensureFullscreenView(
  tabs: RibbonTab[],
  labels: { view: string; show: string; full: string },
  isFullscreen: boolean,
  toggle: () => void,
): RibbonTab[] {
  if (!tabs.length) return tabs
  const fsBtn: RibbonItem = {
    id: 'fullscreen', kind: 'toggle', size: 'large',
    icon: isFullscreen ? <Minimize2 size={22} /> : <Maximize2 size={22} />,
    // `shortcut` ajoute déjà « · F11 » à l'infobulle → tooltip = juste le libellé.
    label: labels.full, tooltip: labels.full, shortcut: 'F11',
    active: isFullscreen, onClick: toggle,
  }
  const isView = (tb: RibbonTab) => tb.backstage == null && (tb.id === 'view' || /^affichage$/i.test(tb.label.trim()))
  const isShow = (g: RibbonGroup) => g.id === 'show' || /^afficher$/i.test(g.label.trim())
  const addToShow = (groups: RibbonGroup[]): RibbonGroup[] => {
    const gi = groups.findIndex(isShow)
    if (gi >= 0) {
      if (groups[gi].items.some(it => it.id === 'fullscreen')) return groups
      const ng = groups.slice(); ng[gi] = { ...ng[gi], items: [...ng[gi].items, fsBtn] }; return ng
    }
    return [{ id: 'show', label: labels.show, items: [fsBtn] }, ...groups]
  }
  const vi = tabs.findIndex(isView)
  if (vi >= 0) {
    const nt = tabs.slice(); nt[vi] = { ...nt[vi], groups: addToShow(nt[vi].groups) }; return nt
  }
  // Aucun onglet Affichage → on le crée (avant les onglets contextuels, s'il y en a).
  const newTab: RibbonTab = { id: 'view', label: labels.view, groups: [{ id: 'show', label: labels.show, items: [fsBtn] }] }
  const ci = tabs.findIndex(tb => tb.contextual)
  const nt = tabs.slice()
  if (ci >= 0) nt.splice(ci, 0, newTab); else nt.push(newTab)
  return nt
}

type ShellProps = ComponentProps<typeof WorkspaceShell>
// On retire les slots remplacés par le ruban. `hideHeaderActions` est déclaré ici
// (le type du paquet npm @kubuno/sdk peut être en retard) et transmis tel quel :
// l'instance HÔTE du WorkspaceShell (résolue par l'import map) le prend en compte.
type OfficeShellProps = Omit<ShellProps, 'menuBar' | 'menus' | 'menuActions' | 'optionsBar' | 'optionsBarHeight'> & {
  ribbon: RibbonTab[]
  activeTabId?: string
  onTabChange?: (id: string) => void
  hideHeaderActions?: boolean
}

export function OfficeShell({ ribbon, activeTabId, onTabChange, hideHeaderActions, theme = WORKSPACE_OFFICE, onBack, ...rest }: OfficeShellProps) {
  const isMobile = useIsMobile()
  const { pathname } = useLocation()
  const { confirm, confirmState, handleConfirm, handleCancel } = useConfirm()

  // Onglet actif du ruban : contrôlé par l'éditeur (activeTabId/onTabChange) si fourni,
  // sinon état local. Il vit ICI car la bande d'onglets est désormais rendue dans la
  // TOPBAR (RibbonTabStrip) et doit rester synchronisée avec le ruban (groupes).
  const [ownTab, setOwnTab] = useState(() => ribbon.find(t => t.visible !== false)?.id ?? '')
  const activeTab = activeTabId ?? ownTab
  const changeTab = useCallback((id: string) => { onTabChange?.(id); if (activeTabId === undefined) setOwnTab(id) }, [onTabChange, activeTabId])

  // ── Plein écran (F11) — bouton garanti dans Affichage → Afficher de TOUT ruban ──
  const { t } = useTranslation()
  const [isFullscreen, setIsFullscreen] = useState(() => typeof document !== 'undefined' && !!document.fullscreenElement)
  useEffect(() => {
    const on = () => setIsFullscreen(!!document.fullscreenElement)
    document.addEventListener('fullscreenchange', on)
    return () => document.removeEventListener('fullscreenchange', on)
  }, [])
  const toggleFullscreen = useCallback(() => {
    if (document.fullscreenElement) document.exitFullscreen?.()
    else document.documentElement.requestFullscreen?.()
  }, [])
  const nribbon = useMemo(() => ensureFullscreenView(ribbon, {
    view: t('office_tab_view',          { defaultValue: 'Affichage' }),
    show: t('office_grp_show',          { defaultValue: 'Afficher' }),
    full: t('office_ribbon_fullscreen', { defaultValue: 'Plein écran' }),
  }, isFullscreen, toggleFullscreen), [ribbon, isFullscreen, t, toggleFullscreen])

  // ── Réduction du ruban + « peek » (façon MS Office) ─────────────────────────────
  // `collapsed` : ruban replié. `peekOpen` : quand replié, un onglet a été cliqué → son
  // contenu s'affiche en flyout flottant jusqu'à la prochaine action hors du ruban.
  // Sur MOBILE : jamais de réduction (ruban = barre du bas).
  // ⚠️ PERSISTANCE PAR SOUS-MODULE : clé dérivée du chemin SANS l'id de document (UUID) →
  // chaque sous-module (documents, spreadsheets, diagrams…) se souvient de SON état.
  // DÉFAUT = DÉPLIÉ (clé absente → `=== '1'` faux).
  const ribbonKey = RIBBON_COLLAPSED_KEY + ':' + (pathname.split('/').filter(Boolean)
    .filter(s => !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(s)).join('/') || 'root')
  const [collapsed, setCollapsed] = useState(() => { try { return localStorage.getItem(ribbonKey) === '1' } catch { return false } })
  const [peekOpen, setPeekOpen] = useState(false)
  // Re-synchronise si l'on change de sous-module (clé différente).
  useEffect(() => { try { setCollapsed(localStorage.getItem(ribbonKey) === '1') } catch { /* ignore */ } }, [ribbonKey])
  const toggleCollapsed = useCallback(() => {
    setCollapsed(c => { const n = !c; try { localStorage.setItem(ribbonKey, n ? '1' : '0') } catch { /* ignore */ } return n })
  }, [ribbonKey])
  // Toute (dé)réduction ferme le peek : au repli, AUCUN onglet n'est sélectionné.
  useEffect(() => { setPeekOpen(false) }, [collapsed])
  useEffect(() => {
    if (isMobile) return
    const onKey = (e: KeyboardEvent) => { if (e.ctrlKey && (e.key === 'F1' || e.code === 'F1')) { e.preventDefault(); toggleCollapsed() } }
    const onStorage = (e: StorageEvent) => { if (e.key === ribbonKey) setCollapsed(e.newValue === '1') }
    window.addEventListener('keydown', onKey)
    window.addEventListener('storage', onStorage)
    return () => { window.removeEventListener('keydown', onKey); window.removeEventListener('storage', onStorage) }
  }, [toggleCollapsed, isMobile, ribbonKey])

  const fileTabId = nribbon.find(t => t.backstage != null)?.id
  const backstageOpen = activeTab === fileTabId && fileTabId != null
  // Réduit + onglet en survol + PAS le backstage → flyout de peek.
  const peeking = !isMobile && collapsed && peekOpen && !backstageOpen
  const closePeek = useCallback(() => setPeekOpen(false), [])

  // Clic sur un onglet (dans la bande) : ouvre le backstage (Fichier) ; sinon, en mode
  // replié, (dé)clenche le peek ; sinon change simplement d'onglet.
  const handleTab = (id: string) => {
    if (id === fileTabId) { changeTab(id); setPeekOpen(false); return }
    if (!isMobile && collapsed) {
      const closing = peekOpen && activeTab === id
      changeTab(id)
      setPeekOpen(!closing)
    } else {
      changeTab(id)
    }
  }
  // Onglet SURLIGNÉ dans la bande : replié sans peek → AUCUN (undefined). Backstage ou
  // peek ou déplié → l'onglet actif.
  const stripActive = (!isMobile && collapsed && !peekOpen && !backstageOpen) ? undefined : activeTab

  // DESKTOP : le bloc d'actions de titre (Enregistrer/Annuler/Rétablir/étoile) et la
  // corbeille quittent le topbar pour la BANDE D'ONGLETS du ruban, juste après « Fichier ».
  // Sur MOBILE il reste dans le topbar (2ᵉ rangée), rendu par le WorkspaceShell.
  const { titleActions, onDelete, deleteConfirm, deleteTitle } = rest as ShellProps
  const triggerDelete = async () => {
    if (!onDelete) return
    const ok = await confirm(deleteConfirm ?? { title: 'Supprimer ?', message: 'Cette action est irréversible.', confirmLabel: 'Supprimer', variant: 'danger' })
    if (ok) onDelete()
  }
  const ribbonActions = (titleActions || onDelete) ? (
    <>
      {titleActions}
      {onDelete && (
        <button onClick={triggerDelete} title={deleteTitle}
          className="p-1.5 rounded hover:bg-white/10 transition-colors flex-shrink-0 text-white/90">
          <Trash2 size={15} />
        </button>
      )}
    </>
  ) : null

  // Desktop : on retire du forward vers le WorkspaceShell (a) le bloc d'actions (déplacé
  // dans la bande d'onglets) et (b) le TITRE + son icône (retirés du topbar : le nom se
  // modifie désormais dans la section Informations). Mobile : on laisse tout passer.
  let forwarded: ShellProps = rest as ShellProps
  if (!isMobile) {
    forwarded = { ...(rest as ShellProps) }
    delete forwarded.titleActions
    delete forwarded.onDelete
    delete forwarded.deleteConfirm
    delete forwarded.deleteTitle
    delete forwarded.title
    delete forwarded.titleIcon
    delete forwarded.onTitleChange
    delete forwarded.onTitleCommit
    delete forwarded.titlePlaceholder
    // BANDE D'ONGLETS DANS LA TOPBAR (desktop) : la rangée dédiée du ruban disparaît
    // (30px gagnés) et, en mode recherche, l'overlay opaque de la topbar recouvre les
    // onglets — masqués sans que rien ne se décale. Hauteur explicite = topbarHeight
    // pour que les onglets s'alignent au BAS de la topbar (l'onglet actif, carte
    // blanche, se raccorde à la rangée de groupes en dessous).
    forwarded.titleSlot = (
      <RibbonTabStrip
        tabs={nribbon} theme={theme} activeId={stripActive} onSelect={handleTab}
        tabStripActions={ribbonActions} inTopbar height={(rest as ShellProps).topbarHeight ?? 40}
        collapsed={collapsed} onToggleCollapsed={toggleCollapsed}
      />
    )
  }

  // Mobile: immersive editing — register a most-specific sidebar config for the
  // exact editor path so the shell hides its bottom nav, waffle FAB and drawer
  // while an editor is open (the mobile ribbon owns the bottom edge). Desktop
  // never sees this config (registered only while isMobile).
  useEffect(() => {
    if (!isMobile) return
    const store = useSidebarStore.getState()
    store.register({ moduleId: 'office-editor', routePrefix: pathname, hideSidebar: true })
    return () => store.unregister('office-editor')
  }, [isMobile, pathname])

  // Bouton « retour » volontairement retiré de TOUS les éditeurs à ruban sur
  // desktop. Sur MOBILE il est rétabli : sans nav basse ni sidebar, la flèche de
  // la topbar est le seul chemin de sortie de l'éditeur.
  const shell = (
    <WorkspaceShell
      {...forwarded}
      {...({ hideHeaderActions } as Partial<ShellProps>)}
      theme={theme}
      onBack={isMobile ? onBack : undefined}
      menuBar={<Ribbon tabs={nribbon} theme={theme} activeTabId={activeTab} onTabChange={changeTab}
                       hideTabs={!isMobile}
                       collapsed={!isMobile && collapsed} peeking={peeking} onClosePeek={closePeek} />}
    />
  )
  // Corbeille du ruban (desktop) → confirmation via ConfirmDialog (jamais de dialog navigateur).
  const withConfirm = (
    <>
      {shell}
      {confirmState && <ConfirmDialog {...confirmState} onConfirm={handleConfirm} onCancel={handleCancel} />}
    </>
  )
  if (!isMobile) return withConfirm
  // Mobile : la barre de commandes du ruban est FIXE en bas de l'écran — on
  // réserve sa hauteur sous l'éditeur (le shell ne pose plus de pb-14 quand la
  // nav basse est masquée par hideSidebar). `ribbon` vide (mode lecture) →
  // pas de barre, pas de réservation : plein écran. ⚠️ Le WRAPPER, lui, est
  // rendu dans les deux cas : basculer lecture ↔ édition ne doit pas changer la
  // forme de l'arbre React, sinon tout l'éditeur (scroller, canvases, listeners
  // de pincement) est démonté/remonté au passage.
  return (
    <div className="flex flex-col h-full min-h-0">
      <div className="flex-1 min-h-0">{shell}</div>
      {ribbon.length > 0 && (
        <div style={{ height: 'calc(48px + env(safe-area-inset-bottom))', flexShrink: 0 }} aria-hidden />
      )}
    </div>
  )
}
