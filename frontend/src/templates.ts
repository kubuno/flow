import type { WorkflowDefinition } from './types'

/** Ready-to-use workflow templates offered on creation (n8n/Make-style gallery). */
export interface WorkflowTemplate {
  id: string
  name: string
  description: string
  icon: string   // lucide icon name
  definition: WorkflowDefinition
}

const node = (id: string, type: string, x: number, y: number, config: Record<string, unknown> = {}, name?: string) =>
  ({ id, type, position: { x, y }, config, ...(name ? { name } : {}) })
const edge = (id: string, source: string, target: string, source_port: string | null = null) =>
  ({ id, source, target, source_port, target_port: null })

export const TEMPLATES: WorkflowTemplate[] = [
  {
    id: 'webhook-email',
    name: 'Webhook → E-mail',
    description: 'Reçoit un webhook et envoie un e-mail de notification.',
    icon: 'Webhook',
    definition: {
      nodes: [
        node('t', 'trigger.webhook', 120, 140, {}, 'Webhook'),
        node('m', 'kubuno.mail.send', 420, 140, {
          to: '{{ $json.email }}',
          subject: 'Nouvelle réception',
          body: 'Données reçues : {{ stringify($json) }}',
        }, 'Envoyer un e-mail'),
      ],
      edges: [edge('e1', 't', 'm')],
    },
  },
  {
    id: 'cron-http-notify',
    name: 'Planifié → API → Notification',
    description: 'Chaque jour, interroge une API REST et notifie l\'utilisateur.',
    icon: 'Clock',
    definition: {
      nodes: [
        node('t', 'trigger.cron', 120, 140, { expression: '0 9 * * *' }, 'Tous les jours 9h'),
        node('h', 'external.http_request', 400, 140, { url: 'https://api.exemple.com/data', method: 'GET' }, 'Requête HTTP'),
        node('n', 'kubuno.notification', 700, 140, { title: 'Rapport', message: '{{ stringify($json.body) }}' }, 'Notifier'),
      ],
      edges: [edge('e1', 't', 'h'), edge('e2', 'h', 'n')],
    },
  },
  {
    id: 'ai-digest',
    name: 'Résumé IA quotidien',
    description: 'Récupère du contenu, le résume avec l\'IA et l\'envoie par e-mail.',
    icon: 'Sparkles',
    definition: {
      nodes: [
        node('t', 'trigger.cron', 100, 160, { expression: '0 8 * * *' }, 'Matin'),
        node('h', 'external.http_request', 360, 160, { url: 'https://exemple.com/articles.json', method: 'GET' }, 'Récupérer'),
        node('ai', 'external.ai', 620, 160, {
          provider: 'anthropic', model: 'claude-haiku-4-5-20251001',
          prompt: 'Résume en 5 points : {{ stringify($json.body) }}',
        }, 'Résumer (IA)'),
        node('m', 'kubuno.mail.send', 900, 160, {
          to: 'moi@exemple.com', subject: 'Votre résumé', body: '{{ $json.text }}',
        }, 'Envoyer'),
      ],
      edges: [edge('e1', 't', 'h'), edge('e2', 'h', 'ai'), edge('e3', 'ai', 'm')],
    },
  },
  {
    id: 'condition-route',
    name: 'Condition → deux chemins',
    description: 'Aiguille selon une condition (vrai / faux) vers deux actions.',
    icon: 'GitBranch',
    definition: {
      nodes: [
        node('t', 'trigger.manual', 120, 200, {}, 'Manuel'),
        node('if', 'logic.if', 380, 200, { value: '{{ $json.montant }}', operator: 'gt', compare: '100' }, 'Montant > 100 ?'),
        node('hi', 'kubuno.notification', 660, 120, { title: 'Gros montant', message: '{{ $json.montant }}' }, 'Alerte'),
        node('lo', 'kubuno.notification', 660, 300, { title: 'Petit montant', message: '{{ $json.montant }}' }, 'Info'),
      ],
      edges: [edge('e1', 't', 'if'), edge('e2', 'if', 'hi', 'true'), edge('e3', 'if', 'lo', 'false')],
    },
  },
  {
    id: 'loop-items',
    name: 'Boucle sur une liste',
    description: 'Parcourt un tableau et traite chaque élément via un sous-workflow.',
    icon: 'Repeat',
    definition: {
      nodes: [
        node('t', 'trigger.manual', 120, 160, {}, 'Manuel'),
        node('sp', 'logic.split', 360, 160, { field: 'items' }, 'Extraire la liste'),
        node('lp', 'flow.loop_items', 620, 160, { workflow_id: '', items: '{{ $json }}' }, 'Pour chaque élément'),
      ],
      edges: [edge('e1', 't', 'sp'), edge('e2', 'sp', 'lp')],
    },
  },
]
