import * as Y from 'yjs'
import {
  Awareness,
  encodeAwarenessUpdate,
  applyAwarenessUpdate,
  removeAwarenessStates,
} from 'y-protocols/awareness'
import { useEffect, useRef } from 'react'
import { useAuthStore } from '@kubuno/sdk'

// Encodage base64 d'un binaire d'awareness (passe par le canal texte Txt du core,
// qui relaie les trames texte telles quelles entre clients d'une même room).
function b64encode(u8: Uint8Array): string {
  let s = ''
  for (let i = 0; i < u8.length; i++) s += String.fromCharCode(u8[i])
  return btoa(s)
}
function b64decode(str: string): Uint8Array {
  const s = atob(str)
  const u8 = new Uint8Array(s.length)
  for (let i = 0; i < s.length; i++) u8[i] = s.charCodeAt(i)
  return u8
}

// Glue PARTAGÉE de collaboration temps réel : relie un `Y.Doc` au service Yjs
// générique du core (`/collab/:room/sync`). Réutilisable par TOUS les éditeurs de
// fichiers kubuno (Office, PaintSharp, Flow, Notes…). Le core relaie/persiste des
// updates binaires opaques ; ici on applique les updates distants au doc et on
// envoie les updates locaux. Reconnexion automatique avec backoff.
//
// Usage typique dans un éditeur :
//   const doc = useMemo(() => new Y.Doc(), [entityId])
//   const collab = useCollab(`office-document:${entityId}`, doc, !!entityId)
//   // ... lier `doc` au contenu (TipTap Collaboration, Y.Map, etc.)

export type CollabStatus = 'connecting' | 'connected' | 'disconnected'

export interface CollabHandle {
  /** Diffuse un message d'awareness (curseur/présence) — texte libre, non persisté. */
  sendAwareness: (json: unknown) => void
  /** Ferme la session. */
  destroy: () => void
  /** État courant de la connexion (lecture seule, mis à jour en interne). */
  statusRef: { current: CollabStatus }
}

function wsUrl(room: string, token: string): string {
  const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  return `${proto}//${window.location.host}/collab/${encodeURIComponent(room)}/sync?token=${encodeURIComponent(token)}`
}

/**
 * Connecte un `Y.Doc` au service collab du core. Renvoie un handle pour
 * l'awareness et la destruction. Le `getToken` permet de rafraîchir le jeton à
 * chaque (re)connexion.
 */
export function connectCollab(
  room: string,
  doc: Y.Doc,
  getToken: () => string | null,
  onStatus?: (s: CollabStatus) => void,
  onAwareness?: (json: unknown) => void,
  /** Appelé à la 1ʳᵉ sync : `empty` = la salle n'avait aucun état (→ seed possible). Une seule fois. */
  onSync?: (empty: boolean) => void,
  /** Instance d'awareness Yjs (curseurs/présence). Si fournie, relayée via le canal texte. */
  awareness?: Awareness,
  /** Construit l'URL WS. Défaut : `/collab/:room/sync`. Permet à un module (ex.
   *  whiteboard) de viser sa propre route tout en réutilisant cette glue. */
  urlBuilder?: (room: string, token: string) => string,
): CollabHandle {
  const buildUrl = urlBuilder ?? wsUrl
  let syncedOnce = false
  let ws: WebSocket | null = null
  let closed = false
  let retry = 0
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null
  const statusRef = { current: 'connecting' as CollabStatus }

  const setStatus = (s: CollabStatus) => { statusRef.current = s; onStatus?.(s) }

  // Updates locaux → serveur (sauf ceux venant du serveur, origin 'remote').
  const onUpdate = (update: Uint8Array, origin: unknown) => {
    if (origin === 'remote') return
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(update as Uint8Array<ArrayBuffer>)
    }
  }
  doc.on('update', onUpdate)

  // ── Awareness (curseurs/présence) ───────────────────────────────────────────
  const sendTxt = (obj: unknown) => {
    if (ws && ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify(obj))
  }
  const sendAwarenessFor = (clients: number[]) => {
    if (!awareness || clients.length === 0) return
    sendTxt({ kbAwareness: b64encode(encodeAwarenessUpdate(awareness, clients)) })
  }
  // Diffuse les changements locaux ; répond aux nouveaux pairs avec notre état.
  const onAwarenessUpdate = (
    { added, updated, removed }: { added: number[]; updated: number[]; removed: number[] },
    origin: unknown,
  ) => {
    if (origin !== 'remote') {
      sendAwarenessFor([...added, ...updated, ...removed])
    } else if (added.length > 0 && awareness) {
      // Un nouveau pair est apparu → on (re)publie notre propre état pour qu'il nous voie.
      sendAwarenessFor([awareness.clientID])
    }
  }
  if (awareness) awareness.on('update', onAwarenessUpdate)

  const open = () => {
    const token = getToken()
    if (!token || closed) return
    setStatus('connecting')
    const sock = new WebSocket(buildUrl(room, token))
    sock.binaryType = 'arraybuffer'
    ws = sock

    sock.onopen = () => {
      retry = 0
      setStatus('connected')
      // Pousser l'état local courant : utile quand un client a « seedé » le doc
      // (document neuf) — l'état devient ainsi partagé/persisté côté serveur.
      const state = Y.encodeStateAsUpdate(doc)
      if (state.length > 2) sock.send(state as Uint8Array<ArrayBuffer>)
      // Publier notre présence courante pour que les pairs déjà connectés nous voient.
      if (awareness && awareness.getLocalState()) sendAwarenessFor([awareness.clientID])
    }
    sock.onmessage = (e) => {
      if (e.data instanceof ArrayBuffer) {
        Y.applyUpdate(doc, new Uint8Array(e.data), 'remote')
      } else if (typeof e.data === 'string') {
        try {
          const m = JSON.parse(e.data)
          if (m && m.type === 'sync') {
            if (!syncedOnce) { syncedOnce = true; onSync?.(!!m.empty) }
          } else if (m && typeof m.kbAwareness === 'string') {
            if (awareness) applyAwarenessUpdate(awareness, b64decode(m.kbAwareness), 'remote')
          } else {
            onAwareness?.(m)
          }
        } catch { /* ignore */ }
      }
    }
    sock.onclose = () => {
      if (ws === sock) ws = null
      setStatus('disconnected')
      if (!closed) scheduleReconnect()
    }
    sock.onerror = () => { try { sock.close() } catch { /* ignore */ } }
  }

  const scheduleReconnect = () => {
    if (closed || reconnectTimer) return
    const delay = Math.min(1000 * 2 ** retry, 15000)
    retry += 1
    reconnectTimer = setTimeout(() => { reconnectTimer = null; open() }, delay)
  }

  open()

  return {
    sendAwareness: (json) => {
      if (ws && ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify(json))
    },
    statusRef,
    destroy: () => {
      closed = true
      if (reconnectTimer) clearTimeout(reconnectTimer)
      doc.off('update', onUpdate)
      if (awareness) {
        awareness.off('update', onAwarenessUpdate)
        // Annonce notre départ aux pairs (curseur retiré) avant de fermer.
        try { removeAwarenessStates(awareness, [awareness.clientID], 'local') } catch { /* ignore */ }
      }
      try { ws?.close() } catch { /* ignore */ }
      ws = null
    },
  }
}

/**
 * Hook React : ouvre une session collab pour `room` liée à `doc` tant que
 * `enabled`. Gère le jeton (authStore) et le nettoyage au démontage.
 */
export function useCollab(
  room: string,
  doc: Y.Doc,
  enabled: boolean,
  opts?: {
    onStatus?: (s: CollabStatus) => void
    onAwareness?: (json: unknown) => void
    onSync?: (empty: boolean) => void
    /** Awareness Yjs (curseurs/présence) — relayée automatiquement si fournie. */
    awareness?: Awareness
    /** Construit l'URL WS (défaut : route collab générique du core). */
    urlBuilder?: (room: string, token: string) => string
  },
): { sendAwareness: (json: unknown) => void } {
  const handleRef = useRef<CollabHandle | null>(null)
  // Callbacks dans une ref → pas de reconnexion à chaque rendu.
  const cbRef = useRef(opts)
  cbRef.current = opts
  const awareness = opts?.awareness

  useEffect(() => {
    if (!enabled) return
    const handle = connectCollab(
      room,
      doc,
      () => useAuthStore.getState().accessToken,
      (s) => cbRef.current?.onStatus?.(s),
      (j) => cbRef.current?.onAwareness?.(j),
      (empty) => cbRef.current?.onSync?.(empty),
      awareness,
      cbRef.current?.urlBuilder,
    )
    handleRef.current = handle
    return () => { handle.destroy(); handleRef.current = null }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [room, enabled, awareness])

  return { sendAwareness: (json) => handleRef.current?.sendAwareness(json) }
}
