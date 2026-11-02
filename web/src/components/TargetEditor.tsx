import { useState } from 'react'
import { useAppStore } from '../store'
import type { Requirement, Side, TargetDraft } from '../types'

interface TargetEditorProps {
  parse: (text: string) => Promise<TargetDraft>
}

const SIDES: Side[] = ['Prefix', 'Suffix']

function describeDraft(draft: TargetDraft): string {
  const parts = [draft.base ?? 'unknown base']
  if (draft.ilvl != null) parts.push(`ilvl ${draft.ilvl}`)
  if (draft.influences.length > 0) parts.push(draft.influences.join(' + '))
  if (draft.catalyst) parts.push(`${draft.catalyst.name} ${draft.catalyst.quality}%`)
  parts.push(`${draft.requirements.length} requirement${draft.requirements.length === 1 ? '' : 's'}`)
  return parts.join(' · ')
}

export function TargetEditor({ parse }: TargetEditorProps) {
  const target = useAppStore((state) => state.target)
  const applyDraft = useAppStore((state) => state.applyDraft)
  const [text, setText] = useState('')
  const [draft, setDraft] = useState<TargetDraft | null>(null)
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

  function patchRequirement(index: number, change: Partial<Requirement>) {
    setDraft((current) =>
      current
        ? { ...current, requirements: current.requirements.map((r, i) => (i === index ? { ...r, ...change } : r)) }
        : current,
    )
  }

  function removeRequirement(index: number) {
    setDraft((current) =>
      current ? { ...current, requirements: current.requirements.filter((_, i) => i !== index) } : current,
    )
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
        <div className="draft">
          <table className="requirements">
            <thead>
              <tr>
                <th>Requirement</th>
                <th>min</th>
                <th>side</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {draft.requirements.map((requirement, index) => (
                <tr key={`${requirement.group}|${index}`}>
                  <td>
                    <input
                      value={requirement.label}
                      onChange={(event) => patchRequirement(index, { label: event.target.value })}
                    />
                  </td>
                  <td>
                    <input
                      type="number"
                      step="any"
                      value={requirement.minValue}
                      onChange={(event) => patchRequirement(index, { minValue: Number(event.target.value) })}
                    />
                  </td>
                  <td>
                    <select
                      value={requirement.side}
                      onChange={(event) => patchRequirement(index, { side: event.target.value as Side })}
                    >
                      {SIDES.map((side) => (
                        <option key={side} value={side}>
                          {side}
                        </option>
                      ))}
                    </select>
                  </td>
                  <td>
                    <button type="button" onClick={() => removeRequirement(index)} aria-label="remove requirement">
                      ×
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>

          <label className="inline-field">
            min hits
            <input
              type="number"
              min="0"
              value={draft.minHits}
              onChange={(event) => setDraft({ ...draft, minHits: Number(event.target.value) })}
            />
          </label>

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
              Buy note: {draft.buyNote.amount} {draft.buyNote.currency} — the goal&rsquo;s buy price is set from the
              current price table.
            </p>
          )}

          <div className="row-actions">
            <button
              type="button"
              disabled={!draft.base}
              onClick={() => applyDraft(draft)}
              title={draft.base ? undefined : 'the parser did not recognise a base item'}
            >
              Apply target
            </button>
            <button type="button" onClick={() => setDraft(null)}>
              Discard draft
            </button>
          </div>
        </div>
      )}
    </section>
  )
}
