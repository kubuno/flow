export type NodeCategory = 'trigger' | 'kubuno' | 'logic' | 'external' | 'code' | 'ai'
export type FieldType = 'text' | 'textarea' | 'expression' | 'number' | 'boolean' | 'select' | 'json' | 'code' | 'credential'

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
  /** Pour un champ credential : types acceptés (CSV). */
  credentialType?: string
}

// ── Credentials ──
export type CredFieldType = 'text' | 'password' | 'select' | 'boolean' | 'number' | 'json'
export interface CredField {
  name: string
  label: string
  type: CredFieldType
  required?: boolean
  placeholder?: string
  help?: string
  default?: unknown
  options?: FieldOption[]
}
export interface CredentialType {
  type: string
  name: string
  icon: string
  category: string
  fields: CredField[]
}
export interface CredentialMeta {
  id: string
  name: string
  type: string
  created_at: string
  updated_at: string
}

export interface PortDef { id: string; label: string }

/** Port d'entrée « sous-nœud » d'un agent IA (modèle, mémoire, outil, parser). */
export interface SubInput { id: string; label: string; kind: string; required?: boolean; multiple?: boolean }

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
  /** Ports de sous-entrée (agent IA), affichés sous le nœud. */
  subInputs?: SubInput[]
  /** Si défini, ce nœud est un sous-nœud fournisseur du type indiqué (se branche par le haut). */
  aiOutput?: string
}

export interface NodePosition { x: number; y: number }

/** Réglages d'exécution par nœud (gestion d'erreur / retry). */
export interface NodeSettings {
  on_error?: 'stop' | 'continue'
  retry_max?: number
  retry_delay_ms?: number
  disabled?: boolean
  note?: string
}

export interface WorkflowNode {
  id: string
  type: string
  name?: string | null
  position: NodePosition
  config: Record<string, unknown>
  settings?: NodeSettings
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

/** Note autocollante (annotation) posée sur le plan de travail. */
export interface StickyNote {
  id: string
  position: NodePosition
  width: number
  height: number
  text: string
  color: string
}

export interface WorkflowDefinition {
  nodes: WorkflowNode[]
  edges: WorkflowEdge[]
  notes?: StickyNote[]
}

/** Une fonction/variable du catalogue d'expressions (autocomplétion). */
export interface ExprHelpItem { name: string; signature?: string; description: string }
export interface ExprHelp { functions: ExprHelpItem[]; variables: ExprHelpItem[] }

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
