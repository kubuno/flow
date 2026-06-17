import { api } from '@kubuno/sdk'
import type { CredentialMeta, CredentialType, Execution, ExprHelp, NodeLog, NodeMeta, Workflow, WorkflowDefinition } from './types'

export const flowApi = {
  // Workflows
  async list(): Promise<Workflow[]> {
    const { data } = await api.get('/flow/workflows')
    return data
  },
  async get(id: string): Promise<Workflow> {
    const { data } = await api.get(`/flow/workflows/${id}`)
    return data
  },
  async openByFile(fileId: string): Promise<Workflow> {
    const { data } = await api.post('/flow/workflows/open-by-file', { file_id: fileId })
    return data
  },
  async create(payload: { name: string; description?: string; definition?: WorkflowDefinition }): Promise<Workflow> {
    const { data } = await api.post('/flow/workflows', payload)
    return data
  },
  async update(id: string, payload: Partial<{ name: string; description: string | null; definition: WorkflowDefinition; status: string; tags: string[] }>): Promise<Workflow> {
    const { data } = await api.put(`/flow/workflows/${id}`, payload)
    return data
  },
  async remove(id: string): Promise<void> {
    await api.delete(`/flow/workflows/${id}`)
  },
  async activate(id: string): Promise<Workflow> {
    const { data } = await api.post(`/flow/workflows/${id}/activate`)
    return data
  },
  async deactivate(id: string): Promise<Workflow> {
    const { data } = await api.post(`/flow/workflows/${id}/deactivate`)
    return data
  },
  async duplicate(id: string): Promise<Workflow> {
    const { data } = await api.post(`/flow/workflows/${id}/duplicate`)
    return data
  },

  // Exécution
  async execute(id: string, triggerData?: unknown): Promise<{ execution_id: string }> {
    const { data } = await api.post(`/flow/workflows/${id}/execute`, triggerData ?? {})
    return data
  },
  async testNode(id: string, nodeId: string, inputData: unknown): Promise<{ success: boolean; output?: unknown; error?: string }> {
    const { data } = await api.post(`/flow/workflows/${id}/nodes/${nodeId}/test`, { input_data: inputData })
    return data
  },

  // Historique
  async executions(id: string): Promise<Execution[]> {
    const { data } = await api.get(`/flow/workflows/${id}/executions`)
    return data
  },
  async executionDetail(id: string): Promise<{ execution: Execution; node_logs: NodeLog[] }> {
    const { data } = await api.get(`/flow/executions/${id}`)
    return data
  },

  // Webhook
  async registerWebhook(id: string, nodeId: string): Promise<{ token: string; path: string }> {
    const { data } = await api.post(`/flow/workflows/${id}/webhook`, { node_id: nodeId })
    return data
  },

  // Catalogue
  async nodeCatalog(): Promise<NodeMeta[]> {
    const { data } = await api.get('/flow/nodes')
    return data
  },
  async expressionHelp(): Promise<ExprHelp> {
    const { data } = await api.get('/flow/expression-help')
    return data
  },

  // Credentials
  async credentialTypes(): Promise<CredentialType[]> {
    const { data } = await api.get('/flow/credential-types')
    return data
  },
  async credentials(): Promise<CredentialMeta[]> {
    const { data } = await api.get('/flow/credentials')
    return data
  },
  async createCredential(payload: { name: string; type: string; data: Record<string, unknown> }): Promise<CredentialMeta> {
    const { data } = await api.post('/flow/credentials', payload)
    return data
  },
  async updateCredential(id: string, payload: { name?: string; data?: Record<string, unknown> }): Promise<CredentialMeta> {
    const { data } = await api.put(`/flow/credentials/${id}`, payload)
    return data
  },
  async deleteCredential(id: string): Promise<void> {
    await api.delete(`/flow/credentials/${id}`)
  },
  async testCredential(payload: { type: string; data: Record<string, unknown> }): Promise<{ ok: boolean | null; message: string }> {
    const { data } = await api.post('/flow/credentials/test', payload)
    return data
  },

  // Import / Export
  async exportWorkflow(id: string): Promise<unknown> {
    const { data } = await api.get(`/flow/workflows/${id}/export`)
    return data
  },
  async importWorkflow(payload: unknown): Promise<Workflow> {
    const { data } = await api.post('/flow/import', payload)
    return data
  },
}

/**
 * Ouvre un flux SSE des logs de nœuds d'une exécution.
 * Le SSE passe par le proxy core (`/api/v1/flow/...`) ; on s'appuie sur le cookie
 * d'accès (withCredentials) car EventSource ne porte pas le header Authorization.
 */
export function streamExecution(
  executionId: string,
  onNode: (log: NodeLog) => void,
  onDone: (status: string) => void,
  onError?: () => void,
): () => void {
  const es = new EventSource(`/api/v1/flow/executions/${executionId}/stream`, { withCredentials: true })
  es.addEventListener('node', (e) => {
    try { onNode(JSON.parse((e as MessageEvent).data)) } catch { /* ignore */ }
  })
  es.addEventListener('done', (e) => {
    onDone((e as MessageEvent).data)
    es.close()
  })
  es.addEventListener('error', () => {
    onError?.()
    es.close()
  })
  return () => es.close()
}
