// Modèle de données du ruban (façon MS Office) — partagé par TOUS les sous-éditeurs
// Office. Chaque sous-éditeur déclare ses onglets ; le composant <Ribbon> fait tout
// le rendu, le style et le responsive. Data-driven : enrichir le ruban = enrichir
// ces structures + <Ribbon>, jamais dupliquer par éditeur.
import type { ReactNode } from 'react'

export type RibbonItemKind =
  | 'button'      // bouton d'action (large = icône+libellé empilés ; small = compact)
  | 'toggle'      // bouton bascule (état actif surligné)
  | 'dropdown'    // liste déroulante (police, taille…) — rendu via @ui Dropdown
  | 'split'       // action principale + petit chevron ouvrant un menu d'options
  | 'gallery'     // rangée d'options/vignettes (styles, couleurs…)
  | 'separator'   // séparateur vertical dans un groupe
  | 'custom'      // échappatoire : ReactNode fourni par l'éditeur (color picker, jauge…)

export interface RibbonOption {
  value: string
  label: string
  icon?: ReactNode
}

export interface RibbonItem {
  id: string
  kind: RibbonItemKind
  label?: string                 // gros bouton : visible ; sinon = tooltip de repli
  icon?: ReactNode
  size?: 'large' | 'small'       // défaut 'small'
  active?: boolean               // toggle / surbrillance
  disabled?: boolean
  tooltip?: string
  shortcut?: string              // affiché dans le tooltip
  onClick?: () => void
  // dropdown / gallery / split
  options?: RibbonOption[]
  value?: string
  onChange?: (value: string) => void
  width?: number                 // largeur dropdown (px)
  splitItems?: RibbonItem[]      // split : entrées du menu déroulant
  // custom
  render?: ReactNode
}

export interface RibbonGroup {
  id: string
  label: string
  items: RibbonItem[]
}

export interface RibbonTab {
  id: string
  label: string
  groups: RibbonGroup[]
  // Onglet CONTEXTUEL (façon Office) : affiché à droite avec un liseré coloré, et
  // n'apparaît que lorsque `visible` est vrai (sélection d'image, de tableau…).
  contextual?: { accent: string; groupLabel?: string }
  visible?: boolean
  // Onglet « Fichier » (Backstage façon Office) : placé en 1ʳᵉ position, stylé avec
  // l'accent de l'app. Quand actif, `backstage` est rendu en OVERLAY plein écran (les
  // `groups` sont ignorés). Une flèche de retour ferme le backstage (sauf si
  // `backstageLocked` = aucun fichier ouvert → on ne peut pas fermer).
  backstage?: ReactNode
  backstageLocked?: boolean
}
