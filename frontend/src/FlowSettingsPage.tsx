import React, { useState } from 'react'
import { Link } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { Workflow as WorkflowIcon, ArrowLeft, ExternalLink, Check } from 'lucide-react'
import { Toggle, Button, Radio } from '@ui'
import { useModulePrefs } from './userPrefs'

// ── Per-user preferences (backend, cross-device via core users.preferences) ─────

interface FlowPrefs {
  gridStyle:    string   // 'dots' | 'lines' | 'none' — canvas background grid
  snapToGrid:   boolean  // snap node positions to the grid
  showMinimap:  boolean  // show the canvas minimap
  defaultZoom:  string   // '50' | '75' | '100' — initial canvas zoom level (%)
  confirmRun:   boolean  // ask for confirmation before executing a workflow
  autoFitView:  boolean  // automatically fit the workflow to the viewport on open
}

const DEFAULT_PREFS: FlowPrefs = {
  gridStyle: 'dots', snapToGrid: false, showMinimap: true,
  defaultZoom: '100', confirmRun: false, autoFitView: true,
}

// ── Mail-style layout helpers ───────────────────────────────────────────────────

function SettingsRow({ label, description, children }: {
  label: string; description?: string; children: React.ReactNode
}) {
  return (
    <div className="flex items-start gap-8 py-4 border-b border-[#e8eaed] last:border-0">
      <div className="w-60 flex-shrink-0">
        <p className="text-sm text-[#202124] font-normal">{label}</p>
        {description && <p className="text-xs text-text-tertiary mt-0.5 leading-relaxed">{description}</p>}
      </div>
      <div className="flex-1">{children}</div>
    </div>
  )
}

function RadioGroup({ options, value, onChange }: {
  options: { value: string; label: string }[]; value: string; onChange: (v: string) => void
}) {
  return (
    <div className="flex flex-col items-start gap-2">
      {options.map(opt => (
        <Radio key={opt.value} checked={value === opt.value} onChange={() => onChange(opt.value)} label={opt.label} />
      ))}
    </div>
  )
}

// ── Préférences tab (per-user) ──────────────────────────────────────────────────

function PreferencesTab() {
  const { t } = useTranslation('flow')
  const { prefs: saved, update } = useModulePrefs<FlowPrefs>('flow', DEFAULT_PREFS)
  const [prefs, setPrefs] = useState<FlowPrefs>(saved)
  const [savedFlag, setSavedFlag] = useState(false)
  const [busy, setBusy] = useState(false)

  const set = <K extends keyof FlowPrefs>(key: K, value: FlowPrefs[K]) =>
    setPrefs(p => ({ ...p, [key]: value }))

  const save = async () => {
    setBusy(true)
    try {
      await update(prefs)
      setSavedFlag(true)
      setTimeout(() => setSavedFlag(false), 2500)
    } finally { setBusy(false) }
  }

  return (
    <div>
      <SettingsRow
        label={t('flow_pref_grid', { defaultValue: 'Grille du canevas' })}
        description={t('flow_pref_grid_desc', { defaultValue: 'Apparence de l\'arrière-plan de la zone d\'édition.' })}
      >
        <RadioGroup
          value={prefs.gridStyle}
          onChange={v => set('gridStyle', v)}
          options={[
            { value: 'dots',  label: t('flow_pref_grid_dots',  { defaultValue: 'Points' }) },
            { value: 'lines', label: t('flow_pref_grid_lines', { defaultValue: 'Lignes' }) },
            { value: 'none',  label: t('flow_pref_grid_none',  { defaultValue: 'Aucune' }) },
          ]}
        />
      </SettingsRow>

      <SettingsRow
        label={t('flow_pref_zoom', { defaultValue: 'Zoom par défaut' })}
        description={t('flow_pref_zoom_desc', { defaultValue: 'Niveau de zoom à l\'ouverture d\'un workflow.' })}
      >
        <RadioGroup
          value={prefs.defaultZoom}
          onChange={v => set('defaultZoom', v)}
          options={[
            { value: '50',  label: '50 %' },
            { value: '75',  label: '75 %' },
            { value: '100', label: '100 %' },
          ]}
        />
      </SettingsRow>

      <SettingsRow label={t('flow_pref_snap', { defaultValue: 'Magnétisme à la grille' })}>
        <label className="flex items-center gap-2 cursor-pointer select-none">
          <Toggle checked={prefs.snapToGrid} onChange={() => set('snapToGrid', !prefs.snapToGrid)} />
          <span className="text-sm text-text-primary">{t('flow_pref_snap_on', { defaultValue: 'Aligner les nœuds sur la grille' })}</span>
        </label>
      </SettingsRow>

      <SettingsRow label={t('flow_pref_minimap', { defaultValue: 'Mini-carte' })}>
        <label className="flex items-center gap-2 cursor-pointer select-none">
          <Toggle checked={prefs.showMinimap} onChange={() => set('showMinimap', !prefs.showMinimap)} />
          <span className="text-sm text-text-primary">{t('flow_pref_minimap_on', { defaultValue: 'Afficher la mini-carte du canevas' })}</span>
        </label>
      </SettingsRow>

      <SettingsRow label={t('flow_pref_fit', { defaultValue: 'Ajustement de la vue' })}>
        <label className="flex items-center gap-2 cursor-pointer select-none">
          <Toggle checked={prefs.autoFitView} onChange={() => set('autoFitView', !prefs.autoFitView)} />
          <span className="text-sm text-text-primary">{t('flow_pref_fit_on', { defaultValue: 'Ajuster le workflow à la fenêtre à l\'ouverture' })}</span>
        </label>
      </SettingsRow>

      <SettingsRow
        label={t('flow_pref_confirm_run', { defaultValue: 'Exécution' })}
        description={t('flow_pref_confirm_run_desc', { defaultValue: 'Demander confirmation avant de lancer un workflow.' })}
      >
        <label className="flex items-center gap-2 cursor-pointer select-none">
          <Toggle checked={prefs.confirmRun} onChange={() => set('confirmRun', !prefs.confirmRun)} />
          <span className="text-sm text-text-primary">{t('flow_pref_confirm_run_on', { defaultValue: 'Confirmer avant l\'exécution' })}</span>
        </label>
      </SettingsRow>

      <div className="pt-5 flex items-center gap-3">
        <Button onClick={save} loading={busy}>
          {savedFlag
            ? <><Check size={14} className="mr-1.5 inline" />{t('flow_settings_saved', { defaultValue: 'Enregistré' })}</>
            : t('flow_settings_save_changes', { defaultValue: 'Enregistrer les modifications' })}
        </Button>
        <Button variant="ghost" onClick={() => setPrefs(saved)}>
          {t('common_cancel', { defaultValue: 'Annuler' })}
        </Button>
      </div>
    </div>
  )
}

// ── About tab ───────────────────────────────────────────────────────────────────

function AboutTab() {
  const { t } = useTranslation('flow')
  return (
    <div className="rounded-xl border border-border overflow-hidden">
      <div className="flex items-center gap-3 px-5 py-4 border-b border-border bg-surface-1">
        <div className="w-10 h-10 rounded-xl bg-indigo-100 flex items-center justify-center shrink-0">
          <WorkflowIcon size={20} className="text-indigo-600" />
        </div>
        <div>
          <p className="text-sm font-semibold text-text-primary">Kubuno Flow</p>
          <p className="text-xs text-text-tertiary">v0.1.0 · {t('flow_official_module', { defaultValue: 'Module officiel' })}</p>
        </div>
        <span className="ml-auto text-xs font-medium px-2 py-0.5 rounded-full bg-orange-100 text-orange-700">Rust</span>
      </div>
      <div className="px-5 py-4 space-y-3">
        <p className="text-sm text-text-secondary leading-relaxed">{t('settings_p1', { defaultValue: 'Flow automatise vos tâches en reliant des déclencheurs et des actions.' })}</p>
        <a href="https://github.com/kubuno/flow" target="_blank" rel="noopener noreferrer"
          className="inline-flex items-center gap-1.5 text-sm text-primary hover:underline">
          <ExternalLink size={13} /> github.com/kubuno/flow
        </a>
      </div>
    </div>
  )
}

// ── Main page (mail-style breadcrumb + tab bar) ─────────────────────────────────

type Tab = 'preferences' | 'about'

export default function FlowSettingsPage() {
  const { t } = useTranslation('flow')
  const [tab, setTab] = useState<Tab>('preferences')

  // No instance-wide (admin) settings exist for flow yet; only per-user prefs + about.
  const tabs: { id: Tab; label: string }[] = [
    { id: 'preferences', label: t('flow_tab_preferences', { defaultValue: 'Préférences' }) },
    { id: 'about',       label: t('flow_tab_about', { defaultValue: 'À propos' }) },
  ]

  return (
    <div className="flex flex-col h-full bg-white overflow-hidden">
      {/* Breadcrumb header */}
      <div className="flex items-center gap-2 px-6 py-2.5 border-b border-[#e8eaed] flex-shrink-0" style={{ background: '#f8f9fa' }}>
        <Link to="/flow" className="flex items-center gap-1.5 text-sm text-[#1a73e8] hover:underline">
          <ArrowLeft size={14} />
          Flow
        </Link>
        <span className="text-text-tertiary text-sm">/</span>
        <div className="flex items-center gap-1.5">
          <WorkflowIcon size={15} className="text-text-secondary" />
          <span className="text-sm text-text-primary">{t('flow_settings_title', { defaultValue: 'Réglages' })}</span>
        </div>
      </div>

      {/* Tab bar (Gmail-style) */}
      <div className="flex items-end border-b border-[#e8eaed] px-4 flex-shrink-0 overflow-x-auto" style={{ background: '#fff' }}>
        {tabs.map(tb => (
          <button key={tb.id} onClick={() => setTab(tb.id)}
            className={`px-4 py-3 text-sm border-b-2 -mb-px transition-colors whitespace-nowrap ${
              tab === tb.id ? 'border-[#1a73e8] text-[#1a73e8] font-medium' : 'border-transparent text-[#5f6368] hover:text-[#202124] hover:bg-[#f1f3f4]'}`}>
            {tb.label}
          </button>
        ))}
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto">
        <div className="max-w-3xl mx-auto px-8 py-6">
          {tab === 'preferences' && <PreferencesTab />}
          {tab === 'about'       && <AboutTab />}
        </div>
      </div>
    </div>
  )
}
