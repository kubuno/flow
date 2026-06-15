import { useEffect, useReducer, useRef, useCallback } from 'react'
import type { ReactNode } from 'react'
import type { Awareness } from 'y-protocols/awareness'

// Présence collaborative partagée : couleur stable par utilisateur + avatars des
// participants présents (lus depuis l'awareness Yjs). Réutilisable par tout éditeur
// branché sur `useCollab(..., { awareness })`.

export interface PresenceUser {
  name:   string
  color:  string
  avatar?: string | null
  id?:    string
}

// Palette type Google Docs (teintes vives lisibles sur fond clair).
const PALETTE = [
  '#1a73e8', '#d93025', '#1e8e3e', '#f9ab00', '#9334e6',
  '#e8710a', '#12b5cb', '#d01884', '#7cb342', '#3949ab',
]

/** Couleur stable dérivée d'un identifiant (hash → palette). */
export function userColor(id: string): string {
  let h = 0
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) >>> 0
  return PALETTE[h % PALETTE.length]
}

export function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean)
  if (parts.length === 0) return '?'
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase()
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase()
}

/** Liste des utilisateurs présents (hors soi), dédupliquée par nom+couleur. */
export function usePresenceUsers(awareness: Awareness | null, selfClientId?: number): PresenceUser[] {
  const [, force] = useReducer((n: number) => n + 1, 0)
  useEffect(() => {
    if (!awareness) return
    awareness.on('change', force)
    return () => awareness.off('change', force)
  }, [awareness])

  if (!awareness) return []
  const seen = new Set<string>()
  const users: PresenceUser[] = []
  awareness.getStates().forEach((state, clientId) => {
    if (clientId === selfClientId) return
    const u = (state as { user?: PresenceUser }).user
    if (!u || !u.name) return
    const key = `${u.name}|${u.color}`
    if (seen.has(key)) return
    seen.add(key)
    users.push(u)
  })
  return users
}

/** Rendu des pastilles d'avatars à partir d'une liste (style Google Docs). */
export function PresenceAvatarList({ users }: { users: PresenceUser[] }) {
  if (users.length === 0) return null
  return (
    <div className="flex items-center -space-x-2 mr-1">
      {users.slice(0, 5).map((u, i) => (
        <div
          key={i}
          title={u.name}
          className="w-7 h-7 rounded-full flex items-center justify-center text-[11px] font-semibold text-white ring-2 ring-white overflow-hidden"
          style={{ backgroundColor: u.color, zIndex: 10 - i }}
        >
          {u.avatar
            ? <img src={u.avatar} alt={u.name} className="w-full h-full object-cover" />
            : initials(u.name)}
        </div>
      ))}
      {users.length > 5 && (
        <div className="w-7 h-7 rounded-full bg-text-tertiary flex items-center justify-center text-[11px] font-semibold text-white ring-2 ring-white">
          +{users.length - 5}
        </div>
      )}
    </div>
  )
}

/** Pastilles d'avatars des participants présents (lit directement l'awareness). */
export function PresenceAvatars({ awareness, selfClientId }: { awareness: Awareness | null; selfClientId?: number }) {
  return <PresenceAvatarList users={usePresenceUsers(awareness, selfClientId)} />
}

// ── Curseurs souris collaboratifs ──────────────────────────────────────────────

/** Renvoie une fonction qui publie la position souris locale (throttlée ~40ms).
 *  Passer `null` pour effacer le curseur (sortie de la zone). */
export function usePublishCursor(awareness: Awareness | null, field = 'cursor') {
  const last = useRef(0)
  return useCallback((cursor: { x: number; y: number } | null) => {
    if (!awareness) return
    if (cursor === null) { awareness.setLocalStateField(field, null); return }
    const now = performance.now()
    if (now - last.current < 40) return
    last.current = now
    awareness.setLocalStateField(field, cursor)
  }, [awareness, field])
}

/** Curseur souris distant : pointeur + étiquette nom, en couleur utilisateur. */
function CursorPointer({ left, top, color, name }: { left: number; top: number; color: string; name: string }) {
  return (
    <div style={{ position: 'absolute', left, top, pointerEvents: 'none', zIndex: 50,
                  transition: 'left .12s ease-out, top .12s ease-out' }}>
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" style={{ display: 'block' }}>
        <path d="M4 2l6.5 16.5L13 12l6.5-2.5z" fill={color} stroke="#fff" strokeWidth="1.3" strokeLinejoin="round" />
      </svg>
      <div style={{ position: 'absolute', left: 14, top: 12, background: color, color: '#fff',
                    fontSize: 11, lineHeight: '16px', padding: '0 6px', borderRadius: 8,
                    whiteSpace: 'nowrap', fontWeight: 600 }}>
        {name}
      </div>
    </div>
  )
}

/** Overlay des curseurs souris distants. `toScreen` mappe le `cursor` publié vers
 *  des px dans le conteneur d'overlay (même repère que les sélections de l'éditeur).
 *  Renvoie null pour ne pas afficher (ex. hors écran). À monter dans un conteneur
 *  `position: relative`. */
export function RemoteCursors({ awareness, selfClientId, toScreen, field = 'cursor' }: {
  awareness: Awareness | null
  selfClientId?: number
  toScreen: (cursor: { x: number; y: number }) => { left: number; top: number } | null
  field?: string
}) {
  const [, force] = useReducer((n: number) => n + 1, 0)
  useEffect(() => {
    if (!awareness) return
    awareness.on('change', force)
    return () => awareness.off('change', force)
  }, [awareness])

  if (!awareness) return null
  const nodes: ReactNode[] = []
  awareness.getStates().forEach((state, clientId) => {
    if (clientId === selfClientId) return
    const s = state as Record<string, unknown> & { user?: PresenceUser }
    const cursor = s[field] as { x: number; y: number } | null | undefined
    if (!s.user || !cursor) return
    const pos = toScreen(cursor)
    if (!pos) return
    nodes.push(<CursorPointer key={clientId} left={pos.left} top={pos.top} color={s.user.color} name={s.user.name} />)
  })
  return <>{nodes}</>
}
