// <Backstage> — vue « Fichier » façon MS Office, partagée par TOUS les sous-éditeurs
// Office. Rail vertical à gauche (couleur d'accent de l'app) = flèche de retour +
// sections (Accueil/Nouveau/Ouvrir/Enregistrer/Exporter/Imprimer/Informations…) ;
// panneau de contenu à droite. Data-driven : chaque éditeur fournit `sections`.
import { useEffect, useState } from 'react'
import type { ReactNode } from 'react'
import type { WorkspaceTheme } from '@kubuno/sdk'
import { fileAccentFor } from './officeThemes'

export interface BackstageSection {
  id:        string
  label:     string
  icon:      ReactNode
  // Panneau de droite (section « vue » : Accueil, Nouveau, Ouvrir, Exporter…). Si
  // absent ET `onSelect` présent → la section déclenche une action immédiate
  // (Imprimer, Fermer…) au lieu d'afficher un panneau.
  content?:  ReactNode
  onSelect?: () => void
  disabled?: boolean
  // Séparateur au-dessus de cette section dans le rail.
  separated?: boolean
}

export function Backstage({ sections, theme, onBack, locked = false, initial }: {
  sections: BackstageSection[]
  theme:    WorkspaceTheme
  onBack:   () => void
  locked?:  boolean          // aucun fichier ouvert → pas de retour possible
  initial?: string           // id de la section affichée au départ
}) {
  const views = sections.filter(s => s.content != null)
  const [active, setActive] = useState<string>(initial ?? views[0]?.id ?? '')
  const cur = sections.find(s => s.id === active) ?? views[0]

  // Échap ferme le backstage (sauf si verrouillé : aucun fichier ouvert).
  useEffect(() => {
    const h = (e: KeyboardEvent) => { if (e.key === 'Escape' && !locked) { e.preventDefault(); onBack() } }
    window.addEventListener('keydown', h)
    return () => window.removeEventListener('keydown', h)
  }, [locked, onBack])

  return (
    <div className="flex h-full w-full" style={{ background: theme.bg, color: theme.text }} data-module="office">
      {/* Rail gauche (couleur d'onglet Fichier, dérivée de l'accent du module) */}
      <div className="flex flex-col w-60 flex-shrink-0 py-2 overflow-y-auto" style={{ background: fileAccentFor(theme.accent), color: '#fff' }}>
        {/* Flèche de retour retirée (la bande d'onglets reste visible → on sort en
            cliquant un autre onglet) ; on CONSERVE l'espace vertical qu'elle créait. */}
        <div className="h-10 mb-2 flex-shrink-0" aria-hidden />

        {sections.map(s => {
          const isActive = active === s.id && s.content != null
          return (
            <div key={s.id}>
              {s.separated && <div className="my-1.5 mx-5 border-t" style={{ borderColor: 'rgba(255,255,255,0.2)' }} />}
              <button disabled={s.disabled}
                onClick={() => { if (s.onSelect) s.onSelect(); else setActive(s.id) }}
                className="flex items-center gap-3 w-full px-5 h-9 text-[13px] text-left disabled:opacity-40 disabled:cursor-default"
                style={{ background: isActive ? 'rgba(255,255,255,0.18)' : 'transparent', fontWeight: isActive ? 600 : 400 }}
                onMouseEnter={e => { if (!isActive && !s.disabled) e.currentTarget.style.background = 'rgba(255,255,255,0.1)' }}
                onMouseLeave={e => { if (!isActive) e.currentTarget.style.background = 'transparent' }}>
                <span className="w-5 flex justify-center flex-shrink-0">{s.icon}</span>
                <span className="truncate">{s.label}</span>
              </button>
            </div>
          )
        })}
      </div>

      {/* Panneau de contenu */}
      <div className="flex-1 min-w-0 overflow-auto">
        {cur?.content}
      </div>
    </div>
  )
}
