// Shared "could not open file" error dialog for every ribbon-based editor. Opening a
// file from the « Fichier » backstage must never fail silently: on error we surface the
// reason in a one-button modal (ConfirmDialog with `hideCancel`). Each editor renders
// `openErrorDialog` and passes `showOpenError` to the `.catch` of its open call.
import type { ReactElement } from 'react'
import { useConfirm } from '@kubuno/sdk'
import { ConfirmDialog } from '@ui'

type TFn = (k: string, o?: Record<string, unknown>) => string

// Extract a human-readable reason from an API/network error (axios error shape:
// backend returns `{ error, message }`; falls back to the JS Error message).
export function openErrorReason(err: unknown): string {
  const e = err as { response?: { data?: { message?: string; error?: string } }; message?: string } | null | undefined
  return (e?.response?.data?.message || e?.response?.data?.error || e?.message || '').trim()
}

export function useOpenError(t: TFn): { showOpenError: (err?: unknown) => void; openErrorDialog: ReactElement | null } {
  const { confirm, confirmState, handleConfirm, handleCancel } = useConfirm()
  const showOpenError = (err?: unknown) => {
    const reason = openErrorReason(err)
    const base = t('office_open_error_body', { defaultValue: "Le fichier n'a pas pu être ouvert." })
    void confirm({
      title:        t('office_open_error_title', { defaultValue: "Impossible d'ouvrir le fichier" }),
      message:      reason ? `${base}\n\n${reason}` : base,
      hideCancel:   true,
      confirmLabel: t('common_ok', { defaultValue: 'OK' }),
      variant:      'danger',
    })
  }
  const openErrorDialog = confirmState
    ? <ConfirmDialog {...confirmState} onConfirm={handleConfirm} onCancel={handleCancel} />
    : null
  return { showOpenError, openErrorDialog }
}
