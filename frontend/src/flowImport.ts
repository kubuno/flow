/**
 * Import de workflows externes (n8n, Make/Integromat) vers le modèle Flow.
 *
 * Les types de nœuds n8n/Make ne correspondent pas 1:1 au catalogue Flow : on
 * mappe les plus courants vers leurs équivalents (webhook/cron/http/if/code…) et
 * on retombe sur un nœud `code.js` portant le type + les paramètres d'origine en
 * commentaire pour les autres — l'import sert d'amorce de migration (structure,
 * positions, noms, liens), à reconfigurer ensuite.
 */
import type { WorkflowDefinition, WorkflowNode, WorkflowEdge } from './types'

export interface ImportedWorkflow { name: string; definition: WorkflowDefinition }

let _c = 0
const uid = (p: string) => `${p}_${Date.now().toString(36)}${(_c++).toString(36)}`

function codeFallback(originalType: string, params: unknown): { type: string; config: Record<string, unknown> } {
  const body = `// Importé — type d'origine : ${originalType}\n// Reconfigurez ce nœud avec un type Flow équivalent.\n// Paramètres d'origine :\n// ${JSON.stringify(params ?? {}, null, 2).replace(/\n/g, '\n// ')}\nreturn items;`
  return { type: 'code.js', config: { code: body } }
}

// ── n8n ──────────────────────────────────────────────────────────────────────

const N8N_MAP: Record<string, string> = {
  'n8n-nodes-base.webhook': 'trigger.webhook',
  'n8n-nodes-base.scheduleTrigger': 'trigger.cron',
  'n8n-nodes-base.cron': 'trigger.cron',
  'n8n-nodes-base.interval': 'trigger.cron',
  'n8n-nodes-base.manualTrigger': 'trigger.manual',
  'n8n-nodes-base.start': 'trigger.manual',
  'n8n-nodes-base.httpRequest': 'external.http_request',
  'n8n-nodes-base.code': 'code.js',
  'n8n-nodes-base.function': 'code.js',
  'n8n-nodes-base.functionItem': 'code.js',
  'n8n-nodes-base.if': 'logic.if',
  'n8n-nodes-base.switch': 'logic.switch',
  'n8n-nodes-base.merge': 'logic.merge',
  'n8n-nodes-base.set': 'logic.set_variable',
  'n8n-nodes-base.editFields': 'logic.set_variable',
  'n8n-nodes-base.filter': 'logic.filter',
  'n8n-nodes-base.wait': 'logic.wait',
  'n8n-nodes-base.splitOut': 'logic.split',
  'n8n-nodes-base.splitInBatches': 'logic.split',
  'n8n-nodes-base.aggregate': 'logic.aggregate',
  'n8n-nodes-base.itemLists': 'logic.aggregate',
  'n8n-nodes-base.noOp': 'logic.stop',
  'n8n-nodes-base.stopAndError': 'logic.stop',
  'n8n-nodes-base.emailSend': 'kubuno.mail.send',
  'n8n-nodes-base.gmail': 'kubuno.mail.send',
}

interface N8nNode { id?: string; name: string; type: string; position?: [number, number]; parameters?: unknown }
interface N8nConn { node: string; type?: string; index?: number }

export function parseN8n(raw: unknown): ImportedWorkflow {
  const obj = raw as { name?: string; nodes?: N8nNode[]; connections?: Record<string, { main?: N8nConn[][] }> }
  if (!obj || !Array.isArray(obj.nodes)) throw new Error('Format n8n invalide (champ « nodes » manquant)')

  const nameToId = new Map<string, string>()
  const nodes: WorkflowNode[] = obj.nodes.map((n, i) => {
    const id = n.id || uid('node')
    nameToId.set(n.name, id)
    const mapped = N8N_MAP[n.type]
    const { type, config } = mapped
      ? { type: mapped, config: (n.parameters as Record<string, unknown>) ?? {} }
      : codeFallback(n.type, n.parameters)
    return {
      id, type, name: n.name,
      position: { x: n.position?.[0] ?? 120 + (i % 5) * 260, y: n.position?.[1] ?? 120 + Math.floor(i / 5) * 160 },
      config,
    }
  })

  const edges: WorkflowEdge[] = []
  for (const [srcName, conn] of Object.entries(obj.connections ?? {})) {
    const source = nameToId.get(srcName); if (!source) continue
    ;(conn.main ?? []).forEach((outs, outIdx) => {
      for (const c of outs ?? []) {
        const target = nameToId.get(c.node); if (!target) continue
        edges.push({ id: uid('edge'), source, target, source_port: outIdx > 0 ? `out_${outIdx}` : null, target_port: null })
      }
    })
  }

  return { name: obj.name || 'Workflow n8n', definition: { nodes, edges } }
}

// ── Make / Integromat ────────────────────────────────────────────────────────

function mapMake(moduleId: string): string | null {
  const m = moduleId.toLowerCase()
  if (m.includes('webhook') || m.includes('gateway')) return 'trigger.webhook'
  if (m.includes('schedule') || m.includes('clock') || m.includes('cron')) return 'trigger.cron'
  if (m.includes('basictrigger') || m.endsWith(':trigger')) return 'trigger.manual'
  if (m.startsWith('http')) return 'external.http_request'
  if (m.includes('email') || m.includes('mail') || m.includes('smtp')) return 'kubuno.mail.send'
  if (m.includes('router')) return 'logic.switch'
  if (m.includes('filter')) return 'logic.filter'
  if (m.includes('setvariable') || m.includes('setvariables')) return 'logic.set_variable'
  if (m.includes('aggregat')) return 'logic.aggregate'
  if (m.includes('iterator') || m.includes('array')) return 'logic.split'
  if (m.includes('sleep') || m.includes('delay')) return 'logic.wait'
  return null
}

interface MakeModule {
  id: number
  module: string
  parameters?: unknown
  mapper?: unknown
  metadata?: { designer?: { name?: string; x?: number; y?: number } }
  routes?: Array<{ flow?: MakeModule[] } | MakeModule[]>
}

export function parseMake(raw: unknown): ImportedWorkflow {
  const obj = raw as { name?: string; flow?: MakeModule[] }
  if (!obj || !Array.isArray(obj.flow)) throw new Error('Format Make invalide (champ « flow » manquant)')

  const nodes: WorkflowNode[] = []
  const edges: WorkflowEdge[] = []
  const layout = { x: 120, y: 120 }

  const walk = (mods: MakeModule[], parentId: string | null) => {
    let prev = parentId
    for (const mod of mods) {
      const id = `node_${mod.id}`
      const mapped = mapMake(mod.module)
      const cfg = (mod.mapper ?? mod.parameters) as Record<string, unknown> | undefined
      const { type, config } = mapped ? { type: mapped, config: cfg ?? {} } : codeFallback(mod.module, cfg)
      nodes.push({
        id, type,
        name: mod.metadata?.designer?.name || mod.module,
        position: { x: mod.metadata?.designer?.x ?? layout.x, y: mod.metadata?.designer?.y ?? layout.y },
        config,
      })
      layout.x += 260
      if (prev) edges.push({ id: uid('edge'), source: prev, target: id, source_port: null, target_port: null })
      if (mod.routes && mod.routes.length) {
        // Routeur : chaque route est une branche partant de ce module.
        let branchY = layout.y + 160
        for (const route of mod.routes) {
          const flow = Array.isArray(route) ? route : (route.flow ?? [])
          const savedY = layout.y; layout.y = branchY
          walk(flow, id)
          layout.y = savedY; branchY += 160
        }
        prev = null // les routes terminent la chaîne linéaire
      } else {
        prev = id
      }
    }
  }
  walk(obj.flow, null)

  return { name: obj.name || 'Scénario Make', definition: { nodes, edges } }
}

/** Détecte le format et renvoie le workflow importé. */
export function parseExternalWorkflow(text: string): { kind: 'n8n' | 'make'; wf: ImportedWorkflow } {
  const obj = JSON.parse(text)
  if (Array.isArray(obj?.flow)) return { kind: 'make', wf: parseMake(obj) }
  if (Array.isArray(obj?.nodes)) return { kind: 'n8n', wf: parseN8n(obj) }
  throw new Error('Format non reconnu (ni n8n « nodes », ni Make « flow »)')
}
