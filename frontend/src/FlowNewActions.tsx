import { i18n, navigate } from '@kubuno/sdk'
/**
 * Items of the sidebar "New" button for Flow — DATA for the project's menu
 * component (`MenuDropdown` from @ui), contributed through the generic
 * 'shell.new-actions' extension point (see entry.ts). Evaluated when the menu
 * opens, so labels are always fresh, without hooks.
 */
import type { MenuItem } from '@ui'
import { Workflow as WorkflowIcon } from 'lucide-react'
// `navigate` is the core's SPA navigation helper for code running outside
// React: the shell hands it the router's real `navigate`.
import { flowApi } from './api'

// Creation failures were already silent in the previous component (empty
// catch); keep that behaviour without unhandled rejections.
const createWorkflow = async () => {
  const wf = await flowApi.create({ name: i18n.t('flow:new_workflow') })
  navigate(`/flow/${wf.id}`)
}

export function flowNewActionItems(): MenuItem[] {
  if (!window.location.pathname.startsWith('/flow')) return []

  return [
    {
      type: 'action',
      label: i18n.t('flow:new_workflow'),
      icon: <WorkflowIcon size={16} className="text-primary" />,
      onClick: () => { createWorkflow().catch(() => {}) },
    },
  ]
}
