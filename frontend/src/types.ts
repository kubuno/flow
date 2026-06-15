export type NodeCategory = 'trigger' | 'kubuno' | 'logic' | 'external' | 'code'
export type FieldType = 'text' | 'textarea' | 'expression' | 'number' | 'boolean' | 'select' | 'json' | 'code'

export interface FieldOption { value: string; label: string }

export interface FieldDef {
  name: string
  label: string
  type: FieldType
  required?: boolean
  placeholder?: string
  help?: string
  default?: unknown
  options?: FieldOption[]
}

export interface PortDef { id: string; label: string }

export interface NodeMeta {
  type: string
  name: string
  description: string
  category: NodeCategory
  icon: string
  color: string
  inputs: number
  outputs: PortDef[]
  fields: FieldDef[]
}

export interface NodePosition { x: number; y: number }

export interface WorkflowNode {
  id: string
  type: string
  name?: string | null
  position: NodePosition
  config: Record<string, unknown>
}

export interface WorkflowEdge {
  id: string
  source: string
  target: string
  source_port?: string | null
  target_port?: string | null
  /** Points de passage (coords monde) pour personnaliser le tracé de la connexion. */
  waypoints?: { x: number; y: number }[]
}

export interface WorkflowDefinition {
  nodes: WorkflowNode[]
  edges: WorkflowEdge[]
}

export interface Workflow {
  id: string
  owner_id: string
  name: string
  description: string | null
  definition: WorkflowDefinition
  status: 'active' | 'inactive'
  execution_count: number
  error_count: number
  last_executed_at: string | null
  last_error: string | null
  tags: string[]
  is_trashed: boolean
  created_at: string
  updated_at: string
}

export interface Execution {
  id: string
  job_id: string | null
  workflow_id: string
  owner_id: string
  status: 'running' | 'success' | 'error' | 'stopped'
  trigger_source: string
  trigger_data: unknown
  duration_ms: number | null
  nodes_executed: number
  nodes_total: number
  error_message: string | null
  started_at: string
  finished_at: string | null
}

export interface NodeLog {
  id: string
  execution_id: string
  node_id: string
  node_type: string
  node_name: string | null
  status: 'success' | 'error' | 'running'
  input_data: unknown
  output_data: unknown
  error_message: string | null
  duration_ms: number | null
  attempt: number
  executed_at: string
}
