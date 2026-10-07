// Boutons « Annuler » / « Rétablir » partagés par tous les éditeurs à ruban (façon
// Office). Placés dans le slot `titleActions` du WorkspaceShell, juste après le bouton
// « Enregistrer ». Style BLANC TRANSLUCIDE identique à SaveButton → s'adapte à la
// couleur de topbar de chaque module. `canUndo`/`canRedo` (optionnels) grisent le
// bouton quand l'historique est vide ; absents → toujours actifs.
import { Undo2, Redo2 } from 'lucide-react'

export function UndoRedoButtons({ onUndo, onRedo, canUndo, canRedo, undoLabel, redoLabel }: {
  onUndo: () => void
  onRedo: () => void
  canUndo?: boolean
  canRedo?: boolean
  undoLabel: string
  redoLabel: string
}) {
  // Même style que les icônes voisines (étoile / corbeille) : fond transparent,
  // survol léger, p-1.5 + icône 15 → taille uniforme dans l'en-tête.
  const btn = 'p-1.5 rounded hover:bg-white/10 transition-colors flex-shrink-0 text-white/90 disabled:opacity-40 disabled:cursor-default'
  return (
    <>
      <button onClick={onUndo} disabled={canUndo === false} title={undoLabel} aria-label={undoLabel} className={btn}>
        <Undo2 size={15} />
      </button>
      <button onClick={onRedo} disabled={canRedo === false} title={redoLabel} aria-label={redoLabel} className={btn}>
        <Redo2 size={15} />
      </button>
    </>
  )
}
