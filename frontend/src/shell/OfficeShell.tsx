// OfficeShell — la chrome COMMUNE à tous les sous-éditeurs Office, qui remplace la
// barre de menus + la toolbar par un RUBAN (façon MS Office). Enveloppe le
// `WorkspaceShell` du core : reprend toutes ses props (topbar, titre, retour, thème,
// actions, statut, rail, bottom/status bar, corps…) et rend le ruban dans le slot
// `menuBar` du shell (donc `menuBar`/`menus`/`optionsBar` ne sont PAS exposés ici :
// le ruban les remplace). Chaque sous-éditeur fournit sa config `ribbon: RibbonTab[]`.
import type { ComponentProps } from 'react'
import { WorkspaceShell, WORKSPACE_OFFICE } from '@kubuno/sdk'
import { Ribbon } from '../ribbon/Ribbon'
import type { RibbonTab } from '../ribbon/types'

type ShellProps = ComponentProps<typeof WorkspaceShell>
// On retire les slots remplacés par le ruban.
type OfficeShellProps = Omit<ShellProps, 'menuBar' | 'menus' | 'menuActions' | 'optionsBar' | 'optionsBarHeight'> & {
  ribbon: RibbonTab[]
  activeTabId?: string
  onTabChange?: (id: string) => void
}

export function OfficeShell({ ribbon, activeTabId, onTabChange, theme = WORKSPACE_OFFICE, onBack, ...rest }: OfficeShellProps) {
  // Bouton « retour » volontairement retiré de TOUS les éditeurs à ruban : on ne
  // transmet pas `onBack` au WorkspaceShell (la prop reste acceptée pour compat des
  // appelants, mais n'affiche plus de flèche dans la topbar).
  void onBack
  return (
    <WorkspaceShell
      {...rest}
      theme={theme}
      menuBar={<Ribbon tabs={ribbon} theme={theme} activeTabId={activeTabId} onTabChange={onTabChange} />}
    />
  )
}
