// Backstage « Fichier » GÉNÉRIQUE (façon Office), partagé par TOUS les sous-éditeurs
// Office. Chaque module fournit son contenu d'accueil (sa StartPage) + ses actions
// (Informations / Exporter / Imprimer / Fermer). `ModuleHome` = page d'accueil SANS
// document : la chrome éditeur avec le backstage ouvert + verrouillé.
import { useCallback, useEffect, useRef, useState } from 'react'
import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { Home, Info, FileDown, Printer, X } from 'lucide-react'
import type { WorkspaceTheme } from '@kubuno/sdk'
import { OfficeShell } from '../shell/OfficeShell'
import { Backstage } from './Backstage'
import type { BackstageSection } from './Backstage'
import type { RibbonTab } from './types'

type TFn = (k: string, o?: Record<string, unknown>) => string

// Libellés traduits du backstage « Fichier » (clés partagées par tous les éditeurs).
export interface BackstageLabels { file: string; home: string; info: string; export: string; print: string; close: string }
export function backstageLabels(t: TFn): BackstageLabels {
  return {
    file:   t('office_bs_file',   { defaultValue: 'Fichier' }),
    home:   t('office_bs_home',   { defaultValue: 'Accueil' }),
    info:   t('office_bs_info',   { defaultValue: 'Informations' }),
    export: t('office_bs_export', { defaultValue: 'Exporter' }),
    print:  t('office_bs_print',  { defaultValue: 'Imprimer' }),
    close:  t('office_bs_close',  { defaultValue: 'Fermer' }),
  }
}

export interface ModuleBackstageExport {
  icon:  ReactNode
  label: string
  sub:   string
  onClick: () => void
}
export interface ModuleBackstageDoc {
  info?:    ReactNode                  // panneau « Informations »
  exports?: ModuleBackstageExport[]    // formats d'export (section « Exporter »)
  onPrint?: () => void                 // section « Imprimer » (action)
  onClose:  () => void                 // section « Fermer » (action)
}

// Construit les sections du backstage : Accueil (toujours) + (si `doc`) Informations /
// Exporter / Imprimer / Fermer. `homeLabel`/`infoLabel`… sont les libellés traduits.
export function moduleBackstageSections(
  labels: { home: string; info: string; export: string; print: string; close: string },
  startContent: ReactNode,
  doc?: ModuleBackstageDoc,
): BackstageSection[] {
  const sections: BackstageSection[] = [
    { id: 'home', label: labels.home, icon: <Home size={17} />, content: <div className="h-full overflow-auto">{startContent}</div> },
  ]
  if (!doc) return sections
  if (doc.info != null) {
    sections.push({ id: 'info', label: labels.info, icon: <Info size={17} />, separated: true, content: doc.info })
  }
  if (doc.exports && doc.exports.length) {
    sections.push({ id: 'export', label: labels.export, icon: <FileDown size={17} />, content: (
      <div className="p-8">
        <h2 className="text-xl font-semibold text-text-primary mb-6">{labels.export}</h2>
        <div className="flex flex-col gap-3">
          {doc.exports.map(e => (
            <button key={e.label} onClick={e.onClick} className="flex items-center gap-3 w-full max-w-md text-left px-4 py-3 rounded-lg border border-border hover:border-primary hover:bg-primary/5 transition-colors">
              <span className="text-primary">{e.icon}</span>
              <span className="flex flex-col"><span className="text-sm font-medium text-text-primary">{e.label}</span><span className="text-sm text-text-tertiary">{e.sub}</span></span>
            </button>
          ))}
        </div>
      </div>
    ) })
  }
  if (doc.onPrint) sections.push({ id: 'print', label: labels.print, icon: <Printer size={17} />, onSelect: doc.onPrint })
  sections.push({ id: 'close', label: labels.close, icon: <X size={17} />, onSelect: doc.onClose, separated: true })
  return sections
}

// Rangées clé/valeur d'un panneau d'informations (propriétés ou statistiques).
export function InfoRows({ rows }: { rows: Array<[string, string | number]> }) {
  if (!rows.length) return <div className="py-8 text-sm text-text-tertiary">—</div>
  return (
    <>
      {rows.map(([k, v]) => (
        <div key={k} className="flex justify-between gap-6 py-1.5 border-b border-border/60 text-sm">
          <span className="text-text-secondary">{k}</span><span className="font-medium text-text-primary text-right">{v}</span>
        </div>
      ))}
    </>
  )
}

// Panneau « Informations » générique (propriétés du document).
export function InfoPanel({ title, rows, subtitle }: { title: string; subtitle?: string; rows: Array<[string, string | number]> }) {
  return (
    <div className="p-8 max-w-2xl">
      <h2 className="text-xl font-semibold text-text-primary mb-1">{title}</h2>
      {subtitle && <p className="text-sm text-text-tertiary mb-6">{subtitle}</p>}
      <InfoRows rows={rows} />
    </div>
  )
}

// Panneau « Informations » à ONGLETS (façon Office récent). Le NOM du document reste
// AU-DESSUS et À L'EXTÉRIEUR des onglets. Onglets : « Général » (propriétés courantes),
// « Résumé », « Statistiques » (compteurs déplacés depuis Général), « Personnalisation ».
// Le contenu de Résumé/Personnalisation (et le complément de Statistiques) sera fourni
// plus tard → placeholder en attendant.
export function BackstageInfo({ title, subtitle, general, stats, summary, custom, onTitleChange, onTitleCommit, extension }: {
  title:     string
  subtitle?: string
  general:   Array<[string, string | number]>   // onglet Général (propriétés)
  stats?:    Array<[string, string | number]>   // onglet Statistiques (compteurs)
  summary?:  ReactNode                           // onglet Résumé (fourni plus tard)
  custom?:   ReactNode                           // onglet Personnalisation (fourni plus tard)
  onTitleChange?: (v: string) => void            // si fourni : le nom devient un champ éditable (renommage)
  onTitleCommit?: () => void                     // validation (blur/Entrée)
  extension?: string                             // extension du fichier (ex. « .kbook »), affichée après le champ
}) {
  const { t } = useTranslation()
  const [tab, setTab] = useState<'general' | 'summary' | 'stats' | 'custom'>('general')
  const titleRef = useRef<HTMLInputElement>(null)
  // Focus AUTOMATIQUE du nom à l'arrivée sur Informations (le panneau est démonté/remonté
  // à chaque ouverture de l'onglet Fichier → focus à chaque venue). rAF : le champ vient
  // d'apparaître dans le portail du backstage.
  useEffect(() => {
    if (!onTitleChange) return
    const r = requestAnimationFrame(() => {
      const el = titleRef.current
      if (!el) return
      el.focus()
      // Curseur en FIN de texte (pas de sélection : taper n'efface plus le nom).
      const n = el.value.length
      el.setSelectionRange(n, n)
    })
    return () => cancelAnimationFrame(r)
  }, [onTitleChange])
  const tabs = [
    { id: 'general', label: t('office_info_tab_general', { defaultValue: 'Général' }) },
    { id: 'summary', label: t('office_info_tab_summary', { defaultValue: 'Résumé' }) },
    { id: 'stats',   label: t('office_info_tab_stats',   { defaultValue: 'Statistiques' }) },
    { id: 'custom',  label: t('office_info_tab_custom',  { defaultValue: 'Personnalisation' }) },
  ] as const
  const placeholder = <div className="py-10 text-sm text-text-tertiary">{t('office_info_soon', { defaultValue: 'Cette section sera bientôt disponible.' })}</div>
  // L'extension garde la MÊME taille et police que le nom (demande user), en teinte atténuée.
  const extEl = extension ? <span className="text-xl font-semibold text-text-tertiary whitespace-pre select-none pointer-events-none">{extension}</span> : null
  return (
    <div className="p-8 max-w-2xl">
      {/* Nom du document : AU-DESSUS et HORS des onglets. Éditable (renommage) si onTitleChange :
          champ auto-dimensionné à la largeur du nom (`field-sizing:content`), extension juste après. */}
      {onTitleChange ? (
        <div className="flex items-baseline mb-1 max-w-full min-w-0">
          <input
            ref={titleRef}
            value={title}
            onChange={e => onTitleChange(e.target.value)}
            onKeyDown={e => { if (e.key === 'Enter') { e.preventDefault(); e.currentTarget.blur() } }}
            onBlur={() => onTitleCommit?.()}
            aria-label={t('office_info_rename', { defaultValue: 'Nom du fichier' })}
            className="text-xl font-semibold text-text-primary bg-transparent outline-none border-b border-transparent focus:border-primary/60 [field-sizing:content] min-w-[1ch] max-w-full"
          />
          {extEl}
        </div>
      ) : (
        <h2 className="text-xl font-semibold text-text-primary mb-1 flex items-baseline min-w-0">
          <span className="truncate">{title}</span>{extEl}
        </h2>
      )}
      {subtitle && <p className="text-sm text-text-tertiary mb-4">{subtitle}</p>}
      <div className="flex gap-1 border-b border-border mb-5">
        {tabs.map(tb => (
          <button key={tb.id} onClick={() => setTab(tb.id)}
            className={`px-3 py-2 text-sm -mb-px border-b-2 transition-colors ${tab === tb.id ? 'border-primary text-primary font-medium' : 'border-transparent text-text-secondary hover:text-text-primary'}`}>
            {tb.label}
          </button>
        ))}
      </div>
      {tab === 'general' && <InfoRows rows={general} />}
      {tab === 'summary' && (summary ?? placeholder)}
      {tab === 'stats'   && (stats && stats.length ? <InfoRows rows={stats} /> : placeholder)}
      {tab === 'custom'  && (custom ?? placeholder)}
    </div>
  )
}

// Page d'accueil d'un sous-module (route sans id) : chrome éditeur avec UNIQUEMENT
// l'onglet Fichier (backstage ouvert + verrouillé : Accueil seul).
export function ModuleHome({ theme, title, titleIcon, fileLabel, homeLabel, startContent, onBack }: {
  theme:        WorkspaceTheme
  title:        string
  titleIcon?:   ReactNode
  fileLabel:    string
  homeLabel:    string
  startContent: ReactNode
  onBack:       () => void
}) {
  const fileTab: RibbonTab = {
    id: 'file', label: fileLabel, groups: [],
    backstage: <Backstage sections={moduleBackstageSections({ home: homeLabel, info: '', export: '', print: '', close: '' }, startContent)} theme={theme} onBack={() => { /* verrouillé */ }} locked />,
    backstageLocked: true,
  }
  return (
    <div className="h-full overflow-hidden flex flex-col">
      <OfficeShell ribbon={[fileTab]} activeTabId="file" chromeless topbarHeight={64} theme={theme} titleIcon={titleIcon} title={title} onBack={onBack}>
        <div className="flex-1" />
      </OfficeShell>
    </div>
  )
}

// Hook partagé : fabrique l'onglet « Fichier » (backstage) d'un ÉDITEUR ouvert et gère
// l'onglet actif du ruban en mode contrôlé. À placer en 1ʳᵉ position du `RibbonTab[]` ;
// brancher `activeTabId`/`onTabChange` sur l'OfficeShell. L'onglet Fichier reste donc
// TOUJOURS visible (il ne disparaît jamais en changeant d'onglet) et, à l'ouverture du
// document, c'est `defaultTab` (et non le backstage) qui est affiché.
export function useFileTab(opts: {
  theme:        WorkspaceTheme
  labels:       BackstageLabels
  startContent: ReactNode
  doc?:         ModuleBackstageDoc
  defaultTab?:  string            // onglet affiché à l'ouverture (défaut : 'home')
  // Identité du document actuellement ouvert (id de route ou de sélection). Quand elle
  // change (= un fichier vient d'être ouvert depuis le backstage), on quitte
  // automatiquement l'onglet « Fichier » pour revenir sur l'onglet d'édition (Accueil).
  openKey?:     string | null
}): { fileTab: RibbonTab; activeTabId: string; onTabChange: (id: string) => void } {
  const home = opts.defaultTab ?? 'home'
  const [active, setActive] = useState(home)
  const prev = useRef(home)
  const onTabChange = useCallback((id: string) => {
    setActive(p => { if (id === 'file' && p !== 'file') prev.current = p; return id })
  }, [])

  // Un nouveau document a été ouvert → sortir du backstage vers l'onglet d'édition.
  // (La navigation ne remonte pas ces éditeurs : sans ça, le backstage resterait affiché.)
  const lastOpenKey = useRef(opts.openKey)
  useEffect(() => {
    if (opts.openKey && opts.openKey !== lastOpenKey.current) setActive(home)
    lastOpenKey.current = opts.openKey
  }, [opts.openKey, home])
  const sections = moduleBackstageSections(opts.labels, opts.startContent, opts.doc)
  // Document ouvert → à chaque OUVERTURE de l'onglet Fichier, on affiche « Informations »
  // par défaut (et non la page d'accueil). Le backstage est démonté en quittant l'onglet
  // (Ribbon : `backstageActive && createPortal`), donc cet `initial` se ré-applique à
  // chaque retour. Sans document ouvert (`ModuleHome`) → pas de section Informations,
  // on garde l'accueil.
  const backstageInitial = opts.doc?.info != null ? 'info' : undefined
  const fileTab: RibbonTab = {
    id: 'file', label: opts.labels.file, groups: [],
    backstage: <Backstage sections={sections} theme={opts.theme} initial={backstageInitial} onBack={() => setActive(prev.current)} />,
  }
  return { fileTab, activeTabId: active, onTabChange }
}
