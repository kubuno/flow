import * as DropdownMenu from '@radix-ui/react-dropdown-menu'
import { Workflow as WorkflowIcon } from 'lucide-react'
import { useLocation, useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { flowApi } from './api'

// Entrée du bouton « Nouveau » du shell (slot `sidebar-new-actions`). Rendue
// dans le DropdownMenu.Root du core → Radix doit être un singleton partagé
// (cf. facade vendor-radix-menu), sinon « `MenuItem` must be used within `Menu` ».
const ITEM =
  'flex items-center gap-2.5 px-3 py-2 text-sm text-text-primary rounded-md ' +
  'hover:bg-surface-2 cursor-pointer outline-none transition-colors'

export default function FlowNewActions() {
  const { t } = useTranslation('flow')
  const location = useLocation()
  const navigate = useNavigate()
  if (!location.pathname.startsWith('/flow')) return null

  const create = async () => {
    try {
      const wf = await flowApi.create({ name: t('new_workflow') })
      navigate(`/flow/${wf.id}`)
    } catch { /* ignore */ }
  }

  return (
    <DropdownMenu.Item onSelect={create} className={ITEM}>
      <WorkflowIcon size={16} className="text-primary" />
      {t('new_workflow')}
    </DropdownMenu.Item>
  )
}
