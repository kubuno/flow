// THE « Presse-papiers » ribbon group — shared by EVERY ribbon editor of Kubuno.
//
// Word puts Clipboard first on the Home tab, and so do we: this helper is the
// single source of that group, so an editor never rebuilds it by hand. Its
// default content is Coller (large) · Couper · Copier · Historique, and every
// piece is overridable — an editor either just wires its own handlers, appends
// its own items (format painter, clear formatting…), or replaces the item list
// outright while keeping the group's identity and position.
//
// The default handlers act on the FOCUSED text field. That works because ribbon
// buttons preventDefault on mousedown, so the editing field keeps focus and
// selection when a button is clicked.
import type { ReactNode, CSSProperties } from 'react'
import { Scissors, Copy, ClipboardList } from 'lucide-react'
import { ModuleServiceRegistry } from '@kubuno/sdk'
import type { RibbonGroup, RibbonItem } from './types'

type TLike = (k: string, o?: Record<string, unknown>) => string

// ── Icône « Coller » ─────────────────────────────────────────────────────────
// Reprise de la famille d'icônes colorées des documents (`documents/ribbon-icons`)
// pour que le bouton Coller soit le MÊME dans tous les modules. SVG autonome (aucune
// dépendance) : le fichier est copié tel quel dans chaque module à ruban.
const BLUE = '#2b579a'      // Word blue
const BLUE_LT = '#41a5ee'   // Office light blue
const GOLD = '#ffb900'      // amber (clipboard body)
const PAPER = '#ffffff'
const GREY = '#605e5c'      // neutral glyph grey

export function PasteIcon({ size, style, className }: { size?: number; style?: CSSProperties; className?: string }) {
  const s = size ?? 22
  return (
    <svg width={s} height={s} viewBox="0 0 24 24" style={style} className={className} aria-hidden>
      <rect x="5" y="4" width="12" height="15" rx="1.4" fill={GOLD} />
      <rect x="8.5" y="2.6" width="5" height="2.8" rx="0.8" fill={GREY} />
      <rect x="9" y="9" width="11" height="12" rx="1.2" fill={PAPER} stroke={BLUE} strokeWidth="1.2" />
      <path d="M11.5 12.5 H17.5 M11.5 15 H17.5 M11.5 17.5 H15" stroke={BLUE_LT} strokeWidth="1.2" strokeLinecap="round" />
    </svg>
  )
}

// ── Comportements par défaut (champ de saisie focalisé) ──────────────────────

/** Paste at the caret of the focused input/textarea (Clipboard API), else execCommand. */
export async function pasteIntoFocused(): Promise<void> {
  const el = document.activeElement as HTMLElement | null
  if (el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA')) {
    try {
      const text = await navigator.clipboard.readText()
      const input = el as HTMLInputElement | HTMLTextAreaElement
      const s = input.selectionStart ?? input.value.length
      const e = input.selectionEnd ?? input.value.length
      input.setRangeText(text, s, e, 'end')
      input.dispatchEvent(new Event('input', { bubbles: true }))
      return
    } catch { /* fall through to execCommand */ }
  }
  document.execCommand('paste')
}

/** Insert plain text at the caret of the focused input/textarea (history fallback). */
function insertTextIntoFocused(text: string): void {
  const el = document.activeElement as HTMLElement | null
  if (el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA')) {
    const input = el as HTMLInputElement | HTMLTextAreaElement
    const s = input.selectionStart ?? input.value.length
    const e = input.selectionEnd ?? input.value.length
    input.setRangeText(text, s, e, 'end')
    input.dispatchEvent(new Event('input', { bubbles: true }))
  }
}

/**
 * Envelope of the cross-module clipboard history (core service). Kept structural
 * so this file stays dependency-free: only `text` is used by the default handler.
 */
export interface ClipboardEnvelope { type: string; text?: string; data?: unknown }

/** True when the host publishes the clipboard history service. */
export function hasClipboardHistoryService(): boolean {
  return typeof ModuleServiceRegistry.get('core', 'openClipboardPane') === 'function'
}

/** Opens the core clipboard pane; resolves with the picked envelope, or null. */
export function openClipboardHistoryPane(types?: string[]): Promise<ClipboardEnvelope | null> {
  return ModuleServiceRegistry.call<Promise<ClipboardEnvelope | null>>('core', 'openClipboardPane', types)
    ?? Promise.resolve(null)
}

// ── Le groupe ────────────────────────────────────────────────────────────────

export interface ClipboardGroupOptions {
  t: TLike
  /** Group label override (defaults to « Presse-papiers »). */
  label?: string
  /** Handlers — each defaults to acting on the focused text field. */
  onPaste?: () => void
  onCut?: () => void
  onCopy?: () => void
  /** Disable a command when the editor knows it cannot run. */
  pasteDisabled?: boolean
  cutDisabled?: boolean
  copyDisabled?: boolean
  /**
   * « Historique » override. Default: open the core clipboard pane and insert the
   * picked entry's text in the focused field. Pass `false` to drop the button
   * (an editor that genuinely has no use for it).
   */
  onHistory?: (() => void) | false
  /** Items APPENDED after the defaults (format painter, clear formatting…). */
  extraItems?: RibbonItem[]
  /** Full item-list override — the group keeps its id, label and first position. */
  items?: RibbonItem[]
  /** Mobile quick bar override (`[]` keeps clipboard out of it, as Word does). */
  mobileQuick?: RibbonItem[]
  /** Paste icon override (an editor with its own icon family). */
  pasteIcon?: ReactNode
}

/**
 * The « Presse-papiers » group — ALWAYS the FIRST group of the Home tab.
 * Undo/Redo do NOT belong here (nor in any other group): they live in the tab
 * strip action block, above the ribbon.
 */
export function clipboardGroup(o: ClipboardGroupOptions): RibbonGroup {
  const { t } = o
  const history: RibbonItem[] = o.onHistory === false ? [] : [{
    id: 'cliphist', kind: 'button', icon: <ClipboardList size={15} />,
    label: t('common_clip_history', { defaultValue: 'Historique' }),
    tooltip: t('common_clip_history_tip', { defaultValue: 'Historique du presse-papiers (tous modules)' }),
    onClick: o.onHistory ?? (() => { void openClipboardHistoryPane().then(env => { if (env?.text) insertTextIntoFocused(env.text) }) }),
  }]
  const items: RibbonItem[] = o.items ?? [
    { id: 'paste', kind: 'button', size: 'large', icon: o.pasteIcon ?? <PasteIcon size={24} />,
      label: t('common_paste', { defaultValue: 'Coller' }), shortcut: 'Ctrl+V', disabled: o.pasteDisabled,
      onClick: o.onPaste ?? (() => { void pasteIntoFocused() }) },
    { id: 'cut', kind: 'button', icon: <Scissors size={15} />,
      label: t('common_cut', { defaultValue: 'Couper' }), shortcut: 'Ctrl+X', disabled: o.cutDisabled,
      onClick: o.onCut ?? (() => { document.execCommand('cut') }) },
    { id: 'copy', kind: 'button', icon: <Copy size={15} />,
      label: t('common_copy', { defaultValue: 'Copier' }), shortcut: 'Ctrl+C', disabled: o.copyDisabled,
      onClick: o.onCopy ?? (() => { document.execCommand('copy') }) },
    ...history,
    ...(o.extraItems ?? []),
  ]
  return {
    id: 'clip',
    label: o.label ?? t('doc_grp_clipboard', { defaultValue: 'Presse-papiers' }),
    ...(o.mobileQuick ? { mobileQuick: o.mobileQuick } : {}),
    items,
  }
}

/**
 * Backwards-compatible shorthand for editors with no clipboard model of their
 * own: the whole group with default (focused-field) behaviour.
 */
export function genericClipboardGroup(t: TLike): RibbonGroup {
  return clipboardGroup({ t })
}
