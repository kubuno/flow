// Bouton « Enregistrer » partagé par tous les éditeurs à ruban (façon Office). Placé
// dans le slot `titleActions` du WorkspaceShell → s'affiche près du titre, JUSTE AVANT
// l'icône corbeille. Style BLANC TRANSLUCIDE : il s'adapte automatiquement à la couleur
// de topbar de chaque module (vert/brique/indigo/cyan/violet/sombre…), sans réglage.
import { Save } from 'lucide-react'

export function SaveButton({ onSave, saving = false, dirty, label }: {
  onSave: () => void
  saving?: boolean
  dirty?: boolean          // si fourni : désactivé quand rien n'a changé
  label:  string
}) {
  const disabled = saving || dirty === false
  return (
    <button onClick={onSave} disabled={disabled} title={label}
      className="flex items-center gap-1 text-xs font-medium text-white bg-white/15 hover:bg-white/25 px-2.5 py-1.5 rounded transition-colors disabled:opacity-40 disabled:cursor-default flex-shrink-0">
      <Save size={14} /> {label}
    </button>
  )
}
