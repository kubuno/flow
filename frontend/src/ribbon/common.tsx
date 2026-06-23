// Briques de ruban communes aux sous-éditeurs Office (groupe « Fichier », etc.).
import { FilePlus, CopyPlus } from 'lucide-react'
import type { RibbonGroup, RibbonItem } from './types'

type TFn = (k: string, o?: Record<string, unknown>) => string

// Groupe « Fichier » standard : Nouveau / Dupliquer (+ items additionnels éventuels).
export function fileGroup(t: TFn, opts: { onNew?: () => void; onDuplicate?: () => void; extra?: RibbonItem[] }): RibbonGroup {
  const items: RibbonItem[] = []
  if (opts.onNew)       items.push({ id: 'new', kind: 'button', icon: <FilePlus size={15} />, label: t('doc_new', { defaultValue: 'Nouveau' }), onClick: opts.onNew })
  if (opts.onDuplicate) items.push({ id: 'dup', kind: 'button', icon: <CopyPlus size={15} />, label: t('doc_duplicate', { defaultValue: 'Dupliquer' }), onClick: opts.onDuplicate })
  if (opts.extra)       items.push(...opts.extra)
  return { id: 'file', label: t('doc_grp_file', { defaultValue: 'Fichier' }), items }
}
