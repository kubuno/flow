/** Bundle MODULE flow — chargé à l'exécution (cf. vite.module.config). */
import { lazy } from 'react'
import { RouteRegistry, CollapseSidebarRegistry, WaffleAppRegistry, FileTypeRegistry, FaviconRegistry, ModuleSettingsRegistry, useToolbarStore, SlotRegistry, SDK_VERSION } from '@kubuno/sdk'
import './index.css'
import './i18n'
import FlowLogo from './FlowLogo'
import FlowNewActions from './FlowNewActions'

export const sdkVersion = SDK_VERSION

export function register() {
  FaviconRegistry.register('flow', '/flow-logo.svg')

  // Type de fichier Kubuno produit par Flow (.kbflw) — filtrage StartPage + icône + ouverture.
  FileTypeRegistry.register({
    moduleId: 'flow', label: 'Flow', icon: 'Workflow',
    mimeTypes: ['application/vnd.kubuno.flow+json'],
    extensions: ['kbflw'],
    open: (f, nav) => { import('./api').then(({ flowApi }) => flowApi.openByFile(f.id).then(wf => nav(`/flow/${wf.id}`)).catch(() => {})) },
  })

  // L'éditeur de workflow occupe toute la largeur : on replie la sidebar du core.
  CollapseSidebarRegistry.add('/flow')

  // The header gear button opens the per-user Flow settings while in /flow.
  ModuleSettingsRegistry.register('flow')

  // Bouton « Nouveau » du shell (comme les autres modules) → crée un workflow.
  SlotRegistry.register('sidebar-new-actions', 'flow', FlowNewActions)

  useToolbarStore.getState().register({
    moduleId:    'flow',
    routePrefix: '/flow',
    noPadding:   true,
  })

  WaffleAppRegistry.register('flow', 'Flow', [
    { id: 'flow', label: 'Flow', Icon: FlowLogo, path: '/flow' },
  ])

  const FlowDashboard    = lazy(() => import('./FlowDashboard'))
  const FlowEditor       = lazy(() => import('./FlowEditor'))
  const FlowSettingsPage = lazy(() => import('./FlowSettingsPage'))

  RouteRegistry.register('flow',          FlowDashboard)
  RouteRegistry.register('flow/settings', FlowSettingsPage)
  RouteRegistry.register('flow/:id',      FlowEditor)
}
