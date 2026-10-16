// The File tab's backstage is the core's (`@ui`); this adapter only gives it the module's File-tab tone.
import type { ComponentProps } from 'react'
import { Backstage as CoreBackstage } from '@ui'
import { fileAccentFor } from './officeThemes'

export type { BackstageSection } from '@ui'

export function Backstage(props: ComponentProps<typeof CoreBackstage>) {
  return <CoreBackstage {...props} theme={{ ...props.theme, fileAccent: fileAccentFor(props.theme.accent) }} />
}
