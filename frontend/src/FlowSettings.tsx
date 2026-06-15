import { useTranslation } from 'react-i18next'
import { Workflow as WorkflowIcon } from 'lucide-react'

export default function FlowSettings() {
  const { t } = useTranslation('flow')
  return (
    <div className="h-full overflow-y-auto bg-surface-1 p-6">
      <div className="max-w-2xl mx-auto">
        <h1 className="text-xl font-semibold text-text-primary flex items-center gap-2 mb-4">
          <WorkflowIcon size={20} className="text-primary" /> {t('settings_title')}
        </h1>
        <div className="bg-surface-0 border border-border rounded-lg p-4 text-sm text-text-secondary">
          <p>{t('settings_p1')}</p>
          <p className="mt-2">{t('settings_p2')}</p>
        </div>
      </div>
    </div>
  )
}
