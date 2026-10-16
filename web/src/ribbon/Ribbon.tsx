// The ribbon is the core's (`@ui`: one implementation for every module that has one). This adapter only gives it the
// module's File-tab tone and its translated collapse tooltips, so the module's screens keep importing it from here.
import type { ComponentProps } from 'react'
import { useTranslation } from 'react-i18next'
import { Ribbon as CoreRibbon, RibbonTabStrip as CoreRibbonTabStrip } from '@ui'
import type { RibbonTheme } from '@ui'
import { fileAccentFor } from './officeThemes'

const withFileAccent = (theme: RibbonTheme): RibbonTheme => ({ ...theme, fileAccent: fileAccentFor(theme.accent) })

export function Ribbon(props: ComponentProps<typeof CoreRibbon>) {
  return <CoreRibbon {...props} theme={withFileAccent(props.theme)} />
}

export function RibbonTabStrip(props: ComponentProps<typeof CoreRibbonTabStrip>) {
  const { t } = useTranslation()
  return (
    <CoreRibbonTabStrip
      expandLabel={t('office_ribbon_expand', { defaultValue: 'Développer le ruban (Ctrl+F1)' })}
      collapseLabel={t('office_ribbon_collapse', { defaultValue: 'Réduire le ruban (Ctrl+F1)' })}
      {...props}
      theme={withFileAccent(props.theme)}
    />
  )
}
