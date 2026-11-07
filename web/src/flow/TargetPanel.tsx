import { useState } from 'react'
import { INFLUENCES, type AbsSpec, type Example, type Rarity } from '../types'
import { errorText, useFlow, useFlowState } from './context'
import { ModPicker } from './ModPicker'

const RARITIES: Rarity[] = ['normal', 'magic', 'rare']

function capitalized(word: string): string {
  return word.charAt(0).toUpperCase() + word.slice(1)
}

/** An integer input's value inside `[min, max]`, or `null` while it is not a number. */
function bounded(text: string, min: number, max: number): number | null {
  const n = Number.parseInt(text, 10)
  return Number.isNaN(n) ? null : Math.min(max, Math.max(min, n))
}

interface TargetPanelProps {
  bases: string[]
  examples: Example[]
}

/** The item the chart is built back from. */
export function TargetPanel({ bases, examples }: TargetPanelProps) {
  const { scope, client, store } = useFlow()
  const spec = useFlowState((state) => state.spec)
  const [paste, setPaste] = useState('')
  const [pasteError, setPasteError] = useState<string | null>(null)

  const update = (patch: Partial<AbsSpec>) => store.getState().setSpec({ ...store.getState().spec, ...patch })
  const options = bases.includes(spec.base) ? bases : [spec.base, ...bases]

  function toggleInfluence(name: string, on: boolean) {
    const rest = spec.influences.filter((i) => i !== name)
    update({ influences: on ? [...rest, name] : rest })
  }

  function applyPaste() {
    client
      .parse(paste)
      .then((view) => {
        const item = view.item
        if (scope.base && item.base !== scope.base) {
          setPasteError(`This page only takes an ${scope.base}.`)
          return
        }
        setPasteError(null)
        update({
          base: item.base,
          itemLevel: item.itemLevel,
          rarity: item.rarity,
          influences: item.influences ?? [],
          prefixes: (item.prefixes ?? []).map((m) => ({ id: m.id })),
          suffixes: (item.suffixes ?? []).map((m) => ({ id: m.id })),
          anyPrefixes: 0,
          anySuffixes: 0,
        })
      })
      .catch((reason: unknown) => setPasteError(errorText(reason)))
  }

  return (
    <section className="panel target-panel">
      <div className="panel-header">
        <h2>Target</h2>
        <span className="hint">the item you want; the chart works back from it</span>
      </div>

      <div className="field">
        <span>Base</span>
        {scope.base ? (
          <strong>{scope.base}</strong>
        ) : (
          <select
            value={spec.base}
            onChange={(event) => update({ base: event.target.value, prefixes: [], suffixes: [] })}
          >
            {options.map((base) => (
              <option key={base} value={base}>
                {base}
              </option>
            ))}
          </select>
        )}
      </div>

      <label className="field">
        <span>Item level</span>
        <input
          type="number"
          min={1}
          max={100}
          value={spec.itemLevel}
          onChange={(event) => {
            const level = bounded(event.target.value, 1, 100)
            if (level !== null) update({ itemLevel: level })
          }}
        />
      </label>

      <label className="field">
        <span>Rarity</span>
        <select value={spec.rarity} onChange={(event) => update({ rarity: event.target.value as Rarity })}>
          {RARITIES.map((rarity) => (
            <option key={rarity} value={rarity}>
              {rarity}
            </option>
          ))}
        </select>
      </label>

      <div className="field">
        <span>Influences</span>
        <div className="influences">
          {INFLUENCES.map((name) => (
            <label key={name} className="check">
              <input
                type="checkbox"
                checked={spec.influences.includes(name)}
                onChange={(event) => toggleInfluence(name, event.target.checked)}
              />
              {capitalized(name)}
            </label>
          ))}
        </div>
      </div>

      <label className="field">
        <span>Any prefixes</span>
        <input
          type="number"
          min={0}
          max={3}
          value={spec.anyPrefixes}
          onChange={(event) => {
            const n = bounded(event.target.value, 0, 3)
            if (n !== null) update({ anyPrefixes: n })
          }}
        />
      </label>
      <label className="field">
        <span>Any suffixes</span>
        <input
          type="number"
          min={0}
          max={3}
          value={spec.anySuffixes}
          onChange={(event) => {
            const n = bounded(event.target.value, 0, 3)
            if (n !== null) update({ anySuffixes: n })
          }}
        />
      </label>
      <p className="hint">
        An "any" slot is a modifier you do not care about: the chart counts it instead of naming it.
      </p>

      <ModPicker />

      <details className="paste">
        <summary>Paste an item</summary>
        {examples.length > 0 && (
          <select
            value=""
            onChange={(event) => setPaste(examples.find((e) => e.id === event.target.value)?.text ?? '')}
          >
            <option value="">Example…</option>
            {examples.map((example) => (
              <option key={example.id} value={example.id}>
                {example.label}
              </option>
            ))}
          </select>
        )}
        <textarea
          rows={10}
          value={paste}
          placeholder="Ctrl+C an item in game and paste it here"
          onChange={(event) => setPaste(event.target.value)}
        />
        <div className="row-actions">
          <button type="button" onClick={applyPaste} disabled={!paste.trim()}>
            Use its modifiers
          </button>
          {pasteError && <span className="error-inline">{pasteError}</span>}
        </div>
      </details>
    </section>
  )
}
