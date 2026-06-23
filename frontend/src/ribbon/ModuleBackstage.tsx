// Backstage « Fichier » GÉNÉRIQUE (façon Office), partagé par TOUS les sous-éditeurs
// Office. Chaque module fournit son contenu d'accueil (sa StartPage) + ses actions
// (Informations / Exporter / Imprimer / Fermer). `ModuleHome` = page d'accueil SANS
// document : la chrome éditeur avec le backstage ouvert + verrouillé.
import { useCallback, useRef, useState } from 'react'
import type { ReactNode } from 'react'
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
              <span className="flex flex-col"><span className="text-sm font-medium text-text-primary">{e.label}</span><span className="text-xs text-text-tertiary">{e.sub}</span></span>
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

// Panneau « Informations » générique (propriétés du document).
export function InfoPanel({ title, rows, subtitle }: { title: string; subtitle?: string; rows: Array<[string, string | number]> }) {
  return (
    <div className="p-8 max-w-2xl">
      <h2 className="text-xl font-semibold text-text-primary mb-1">{title}</h2>
      {subtitle && <p className="text-sm text-text-tertiary mb-6">{subtitle}</p>}
      {rows.map(([k, v]) => (
        <div key={k} className="flex justify-between gap-6 py-1.5 border-b border-border/60 text-sm">
          <span className="text-text-secondary">{k}</span><span className="font-medium text-text-primary text-right">{v}</span>
        </div>
      ))}
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
}): { fileTab: RibbonTab; activeTabId: string; onTabChange: (id: string) => void } {
  const home = opts.defaultTab ?? 'home'
  const [active, setActive] = useState(home)
  const prev = useRef(home)
  const onTabChange = useCallback((id: string) => {
    setActive(p => { if (id === 'file' && p !== 'file') prev.current = p; return id })
  }, [])
  const sections = moduleBackstageSections(opts.labels, opts.startContent, opts.doc)
  const fileTab: RibbonTab = {
    id: 'file', label: opts.labels.file, groups: [],
    backstage: <Backstage sections={sections} theme={opts.theme} onBack={() => setActive(prev.current)} />,
  }
  return { fileTab, activeTabId: active, onTabChange }
}
