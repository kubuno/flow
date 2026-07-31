// Bouton « Enregistrer » partagé par tous les éditeurs à ruban (façon Office). Placé
// dans le slot `titleActions` du WorkspaceShell → s'affiche près du titre, JUSTE AVANT
// les boutons Annuler/Rétablir, l'étoile et la corbeille.
//
// ICÔNE SEULE, du même style que ces voisins (fond transparent, survol léger,
// p-1.5 + icône 15) : un bouton plein à libellé attirait l'œil plus que l'action
// ne le mérite dans une barre de titre, et rompait l'alignement de la rangée.
// Le libellé reste dans le `title`/`aria-label`, donc l'infobulle du shell l'affiche.
//
// L'état d'enregistrement (jadis affiché en toutes lettres sous le titre) est désormais
// un petit BADGE épinglé en bas à droite de l'icône :
//   • enregistrement en cours → deux demi-arcs fléchés qui tournent (RefreshCw + spin) ;
//   • changement non enregistré → un point qui clignote (enregistrement possible) ;
//   • enregistré → une coche.
import { Save, Check, RefreshCw } from 'lucide-react'

export function SaveButton({ onSave, saving = false, dirty, label }: {
  onSave: () => void
  saving?: boolean
  dirty?: boolean          // si fourni : désactivé quand rien n'a changé + pilote le badge
  label:  string
}) {
  const disabled = saving || dirty === false

  // Badge d'état. Sans signal `dirty` (éditeurs en autosave), on n'a que deux états
  // réels : « en cours » et « enregistré » — le point clignotant n'apparaît que là où
  // un vrai état non-enregistré est remonté. Chaque badge est posé sur une PASTILLE
  // BLANCHE (les rubans ont un fond coloré/sombre → sans elle, les glyphes se noient).
  const badge = saving
    ? <RefreshCw size={9} strokeWidth={2.75} className="animate-spin text-text-secondary" />
    : dirty
      ? <span className="block h-[7px] w-[7px] rounded-full bg-warning animate-pulse" />
      : <Check size={9} strokeWidth={3.5} className="text-success" />

  return (
    <button
      onClick={onSave}
      disabled={disabled}
      title={label}
      aria-label={label}
      className="relative p-1.5 rounded hover:bg-white/10 transition-colors flex-shrink-0 text-white/90 disabled:opacity-40 disabled:cursor-default"
    >
      <Save size={15} />
      <span
        aria-hidden
        className="absolute -bottom-1 -right-1 flex h-3.5 w-3.5 items-center justify-center rounded-full bg-white ring-1 ring-black/10 shadow-sm pointer-events-none"
      >
        {badge}
      </span>
    </button>
  )
}
