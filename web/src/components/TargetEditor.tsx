import { useEffect, useState } from 'react'
import { draftToTarget, minValueFor, targetSignature, targetToDraft, type TierMode } from '../problem'
import { rootTarget, useAppStore } from '../store'
import type { Requirement, Rarity, Side, TargetDraft, TierJson } from '../types'

const SIDES: Side[] = ['Prefix', 'Suffix']
const RARITIES: Rarity[] = ['Normal', 'Magic', 'Rare']

export interface ModTierLookup {
  (base: string, group: string, side: Side): Promise<TierJson[]>
}

interface RequirementsEditorProps {
  draft: TargetDraft
  onChange: (draft: TargetDraft) => void
  /** the tier picker's data source; without it the rows keep their typed minimum */
  modTiers?: ModTierLookup
}

function describeDraft(draft: TargetDraft): string {
  const parts = [draft.base ?? 'unknown base']
  if (draft.ilvl != null) parts.push(`ilvl ${draft.ilvl}`)
  if (draft.influences.length > 0) parts.push(draft.influences.join(' + '))
  if (draft.catalyst) parts.push(`${draft.catalyst.name} ${draft.catalyst.quality}%`)
  parts.push(`${draft.requirements.length} requirement${draft.requirements.length === 1 ? '' : 's'}`)
  return parts.join(' · ')
}

type Mode = TierMode | 'custom'

function modeFor(requirement: Requirement, tier: TierJson | undefined): Mode {
  if (!tier) return 'custom'
  if (requirement.minValue === tier.min) return 'any'
  if (requirement.minValue === tier.max) return 'perfect'
  return 'custom'
}

/**
 * One requirement row: tier picker (fed by `mod_tiers`), roll mode and the
 * sources badge that says how the mod is obtained at all.
 */
function RequirementRow({
  requirement,
  base,
  modTiers,
  onPatch,
  onRemove,
}: {
  requirement: Requirement
  base: string | null
  modTiers?: ModTierLookup
  onPatch: (change: Partial<Requirement>) => void
  onRemove: () => void
}) {
  const [tiers, setTiers] = useState<TierJson[]>([])

  useEffect(() => {
    if (!modTiers || !base || requirement.group === '') {
      setTiers([])
      return
    }
    let cancelled = false
    modTiers(base, requirement.group, requirement.side)
      .then((list) => {
        if (!cancelled) setTiers(list)
      })
      .catch(() => {
        if (!cancelled) setTiers([])
      })
    return () => {
      cancelled = true
    }
  }, [base, modTiers, requirement.group, requirement.side])

  const tier = tiers.find((candidate) => candidate.id === requirement.accepted[0]) ?? tiers[0]
  const mode = modeFor(requirement, tier)
  const source = requirement.sources?.[0]
  const badge = source && source !== 'Native' ? `only from: ${source}` : null

  function selectTier(id: string) {
    const next = tiers.find((candidate) => candidate.id === id)
    if (!next) return
    onPatch({
      accepted: [id],
      label: next.text,
      minValue: minValueFor(next, mode === 'perfect' ? 'perfect' : 'any'),
    })
  }

  function selectMode(value: Mode) {
    if (!tier || value === 'custom') return
    onPatch({ minValue: minValueFor(tier, value) })
  }

  return (
    <tr>
      <td>
        <input value={requirement.label} onChange={(event) => onPatch({ label: event.target.value })} />
        {badge && <div className="source-badge">{badge}</div>}
      </td>
      <td>
        {tiers.length > 0 ? (
          <select value={tier?.id ?? ''} onChange={(event) => selectTier(event.target.value)}>
            {tiers.map((candidate) => (
              <option key={candidate.id} value={candidate.id}>
                ilvl {candidate.ilvl} · {candidate.min}–{candidate.max} · {candidate.text}
              </option>
            ))}
          </select>
        ) : (
          <span className="hint">{requirement.group}</span>
        )}
      </td>
      <td>
        <select value={mode} onChange={(event) => selectMode(event.target.value as Mode)} aria-label="roll">
          <option value="any">any roll</option>
          <option value="perfect">perfect</option>
          <option value="custom" disabled>
            typed
          </option>
        </select>
      </td>
      <td>
        <input
          type="number"
          step="any"
          value={requirement.minValue}
          onChange={(event) => onPatch({ minValue: Number(event.target.value) })}
        />
      </td>
      <td>
        <select
          value={requirement.side}
          onChange={(event) => onPatch({ side: event.target.value as Side })}
        >
          {SIDES.map((side) => (
            <option key={side} value={side}>
              {side}
            </option>
          ))}
        </select>
      </td>
      <td>
        <button type="button" onClick={onRemove} aria-label="remove requirement">
          ×
        </button>
      </td>
    </tr>
  )
}

/** The editable body of a target: rarity, item level, requirements and minimum hits. */
export function RequirementsEditor({ draft, onChange, modTiers }: RequirementsEditorProps) {
  function patchRequirement(index: number, change: Partial<Requirement>) {
    onChange({
      ...draft,
      requirements: draft.requirements.map((requirement, i) =>
        i === index ? { ...requirement, ...change } : requirement,
      ),
    })
  }

  return (
    <div className="draft">
      <div className="price-row">
        <label>
          rarity
          <select
            value={draft.rarity}
            onChange={(event) => onChange({ ...draft, rarity: event.target.value as Rarity })}
          >
            {RARITIES.map((rarity) => (
              <option key={rarity} value={rarity}>
                {rarity}
              </option>
            ))}
          </select>
        </label>
        <label>
          ilvl
          <input
            type="number"
            min="1"
            value={draft.ilvl ?? 1}
            onChange={(event) => onChange({ ...draft, ilvl: Number(event.target.value) })}
          />
        </label>
      </div>

      <table className="requirements">
        <thead>
          <tr>
            <th>Requirement</th>
            <th>tier</th>
            <th>roll</th>
            <th>min</th>
            <th>side</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          {draft.requirements.map((requirement, index) => (
            <RequirementRow
              key={`${requirement.group}|${index}`}
              requirement={requirement}
              base={draft.base}
              modTiers={modTiers}
              onPatch={(change) => patchRequirement(index, change)}
              onRemove={() =>
                onChange({ ...draft, requirements: draft.requirements.filter((_, i) => i !== index) })
              }
            />
          ))}
        </tbody>
      </table>

      <label className="inline-field">
        min hits
        <input
          type="number"
          min="0"
          value={draft.minHits}
          onChange={(event) => onChange({ ...draft, minHits: Number(event.target.value) })}
        />
      </label>
    </div>
  )
}

interface TargetEditorProps {
  parse: (text: string) => Promise<TargetDraft>
  modTiers?: ModTierLookup
}

/** Paste a tooltip, edit what the parser found, and make it the root good. */
export function TargetEditor({ parse, modTiers }: TargetEditorProps) {
  const target = useAppStore(rootTarget)
  const applyDraft = useAppStore((state) => state.applyDraft)
  const addGoodFromDraft = useAppStore((state) => state.addGoodFromDraft)
  const [text, setText] = useState('')
  // Seeded from the root good so its rows stay editable after a reload.
  const [draft, setDraft] = useState<TargetDraft | null>(() => (target ? targetToDraft(target) : null))
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  async function onParse() {
    if (text.trim() === '') return
    setBusy(true)
    setError(null)
    try {
      setDraft(await parse(text))
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      setBusy(false)
    }
  }

  const unchanged =
    draft != null && draft.buyNote == null && targetSignature(draftToTarget(draft)) === targetSignature(target)
  const applyTitle = !draft?.base
    ? 'the parser did not recognise a base item'
    : unchanged
      ? 'this is the target already in use'
      : 'applying a target replaces the market built for the previous one'

  function apply(current: TargetDraft) {
    applyDraft(current)
    // The buy note has been turned into the root good's buy price; re-applying must not reset the market.
    setDraft({ ...current, buyNote: null })
  }

  return (
    <section className="panel target-editor">
      <header className="panel-header">
        <h2>Target</h2>
        {target ? (
          <span className="target-summary">
            {target.rarity} {target.base}
            {target.ilvlMin ? ` · ilvl ${target.ilvlMin}` : ''}
            {target.influences.length > 0 ? ` · ${target.influences.join(' + ')}` : ''}
            {target.catalyst ? ` · ${target.catalyst.name} ${target.catalyst.quality}%` : ''}
          </span>
        ) : (
          <span className="hint">paste an item to begin</span>
        )}
      </header>

      <textarea
        value={text}
        onChange={(event) => setText(event.target.value)}
        placeholder="Paste the item text copied in Path of Exile (Ctrl+C on the item)."
        rows={8}
      />
      <div className="row-actions">
        <button type="button" onClick={onParse} disabled={busy || text.trim() === ''}>
          {busy ? 'Parsing…' : 'Parse item'}
        </button>
        {draft && <span className="hint">{describeDraft(draft)}</span>}
      </div>

      {error && <p className="error-inline">{error}</p>}

      {draft && (
        <>
          <RequirementsEditor draft={draft} onChange={setDraft} modTiers={modTiers} />

          {draft.unmatched.length > 0 && (
            <div>
              <h3>Not understood</h3>
              <ul className="unmatched">
                {draft.unmatched.map((line) => (
                  <li key={line}>{line}</li>
                ))}
              </ul>
            </div>
          )}
          {draft.warnings.length > 0 && (
            <ul className="warnings">
              {draft.warnings.map((warning) => (
                <li key={warning}>{warning}</li>
              ))}
            </ul>
          )}
          {draft.buyNote && (
            <p className="hint">
              Buy note: {draft.buyNote.amount} {draft.buyNote.currency} — the root good&rsquo;s buy price is set from the
              current price table.
            </p>
          )}

          <div className="row-actions">
            <button type="button" disabled={!draft.base || unchanged} onClick={() => apply(draft)} title={applyTitle}>
              Apply target
            </button>
            {target && (
              <button
                type="button"
                disabled={!draft.base || unchanged}
                onClick={() => addGoodFromDraft(draft)}
                title="keep the current target and add this item as another good (a brick or an input) to price"
              >
                Add as good
              </button>
            )}
            <button type="button" onClick={() => setDraft(target ? targetToDraft(target) : null)}>
              {target ? 'Revert edits' : 'Discard draft'}
            </button>
          </div>
        </>
      )}
    </section>
  )
}
