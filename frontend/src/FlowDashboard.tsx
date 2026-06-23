// Flow home page (route without id): the editor chrome with only the locked
// "Fichier" backstage open, showing the shared FlowStartContent (recents + browse).
import { useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { Workflow as WorkflowIcon } from 'lucide-react'
import { THEME_FLOW } from './ribbon/officeThemes'
import { ModuleHome } from './ribbon/ModuleBackstage'
import FlowStartContent from './FlowStartContent'

export default function FlowDashboard() {
  const { t } = useTranslation('flow')
  const navigate = useNavigate()

  return (
    <ModuleHome
      theme={THEME_FLOW}
      title={t('title', { defaultValue: 'Flow' })}
      titleIcon={<WorkflowIcon size={16} className="text-white/90 flex-shrink-0" />}
      fileLabel={t('office_bs_file', { defaultValue: 'Fichier' })}
      homeLabel={t('office_bs_home', { defaultValue: 'Accueil' })}
      onBack={() => navigate('/')}
      startContent={<FlowStartContent />}
    />
  )
}
