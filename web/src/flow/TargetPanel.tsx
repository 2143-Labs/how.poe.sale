import { useState } from 'react'
import { INFLUENCES, type AbsSpec, type Example, type ModOption, type Rarity, type SlotSpec } from '../types'
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

/** The item the chart starts from: the root class, and the modifiers watched on top of it. */
export function TargetPanel({ bases, examples }: TargetPanelProps) {
  const { scope, client, store } = useFlow()
  const root = useFlowState((state) => state.root)
  const watch = useFlowState((state) => state.watch)
  const [paste, setPaste] = useState('')
  const [pasteError, setPasteError] = useState<string | null>(null)

  const update = (patch: Partial<AbsSpec>) => store.getState().setRoot({ ...store.getState().root, ...patch })
  const options = bases.includes(root.base) ? bases : [root.base, ...bases]
  const chosen = [...root.prefixes, ...root.suffixes].map((slot) => slot.id)
  const families = [...root.prefixes, ...root.suffixes].filter((slot) => slot.orBetter).map((slot) => slot.id)

  function toggleInfluence(name: string, on: boolean) {
    const rest = root.influences.filter((i) => i !== name)
    update({ influences: on ? [...rest, name] : rest })
  }

  function toggleSlot(option: ModOption, on: boolean) {
    const current = store.getState().root
    const key = option.side === 'prefix' ? 'prefixes' : 'suffixes'
    const rest = current[key].filter((slot) => slot.id !== option.id)
    const next: SlotSpec[] = on ? [...rest, { id: option.id, orBetter: true }] : rest
    update({ [key]: next } as Partial<AbsSpec>)
  }

  function removeSlot(id: string) {
    const current = store.getState().root
    update({
      prefixes: current.prefixes.filter((slot) => slot.id !== id),
      suffixes: current.suffixes.filter((slot) => slot.id !== id),
    })
  }

  function toggleFamily(id: string) {
    const flip = (list: SlotSpec[]) =>
      list.map((slot) => (slot.id === id ? { ...slot, orBetter: !slot.orBetter } : slot))
    const current = store.getState().root
    update({ prefixes: flip(current.prefixes), suffixes: flip(current.suffixes) })
  }

  function toggleWatch(option: ModOption, on: boolean) {
    const current = store.getState().watch
    const rest = current.filter((slot) => slot.id !== option.id)
    store.getState().setWatch(on ? [...rest, { id: option.id, orBetter: true }] : rest)
  }

  function toggleWatchFamily(id: string) {
    store
      .getState()
      .setWatch(watch.map((slot) => (slot.id === id ? { ...slot, orBetter: !slot.orBetter } : slot)))
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
          prefixes: (item.prefixes ?? []).map((entry) => ({ id: entry.id })),
          suffixes: (item.suffixes ?? []).map((entry) => ({ id: entry.id })),
          anyPrefixes: 0,
          anySuffixes: 0,
          corrupted: item.corrupted === true,
        })
      })
      .catch((reason: unknown) => setPasteError(errorText(reason)))
  }

  return (
    <section className="panel target-panel">
      <div className="panel-header">
        <h2>Root</h2>
        <span className="hint">the item the chart starts from; push it forward or read it back</span>
      </div>

      <div className="field">
        <span>Base</span>
        {scope.base ? (
          <strong>{scope.base}</strong>
        ) : (
          <select
            value={root.base}
            onChange={(event) =>
              update({ base: event.target.value, prefixes: [], suffixes: [], corrupted: false })
            }
          >
            {options.map((base) => (
              <option key={base} value={base}>
                {base}
              </option>
            ))}
          </select>
        )}
      </div>

      <div className="field">
        <span>Item level</span>
        <input
          type="number"
          min={1}
          max={100}
          value={root.itemLevel}
          onChange={(event) => {
            const level = bounded(event.target.value, 1, 100)
            if (level !== null) update({ itemLevel: level })
          }}
        />
      </div>

      <div className="field">
        <span>Rarity</span>
        <select value={root.rarity} onChange={(event) => update({ rarity: event.target.value as Rarity })}>
          {RARITIES.map((rarity) => (
            <option key={rarity} value={rarity}>
              {capitalized(rarity)}
            </option>
          ))}
        </select>
      </div>

      <div className="field">
        <span>Corrupted</span>
        <label className="check">
          <input
            type="checkbox"
            checked={root.corrupted}
            onChange={(event) => update({ corrupted: event.target.checked })}
          />
          corrupted
        </label>
      </div>

      <div className="field influences">
        <span>Influences</span>
        {INFLUENCES.map((name) => (
          <label key={name} className="check">
            <input
              type="checkbox"
              checked={root.influences.includes(name)}
              onChange={(event) => toggleInfluence(name, event.target.checked)}
            />
            {capitalized(name)}
          </label>
        ))}
      </div>

      <div className="field">
        <span>Untracked</span>
        <div className="influences">
          <label className="check">
            any prefixes
            <input
            type="number"
              min={0}
              max={3}
              value={root.anyPrefixes}
              onChange={(event) => {
                const n = bounded(event.target.value, 0, 3)
                if (n !== null) update({ anyPrefixes: n })
              }}
            />
          </label>
          <label className="check">
            any suffixes
            <input
              type="number"
              min={0}
              max={3}
              value={root.anySuffixes}
              onChange={(event) => {
                const n = bounded(event.target.value, 0, 3)
                if (n !== null) update({ anySuffixes: n })
              }}
            />
          </label>
        </div>
      </div>

      <ModPicker
        title="Tracked modifiers"
        selected={chosen}
        onToggle={toggleSlot}
        onRemove={removeSlot}
        family={{ ids: families, onToggleFamily: toggleFamily }}
        empty='None: every modifier is an "any" slot.'
      />

      <ModPicker
        title="Also track"
        selected={watch.map((slot) => slot.id)}
        onToggle={toggleWatch}
        onRemove={(id) => store.getState().setWatch(watch.filter((slot) => slot.id !== id))}
        family={{ ids: watch.filter((slot) => slot.orBetter).map((slot) => slot.id), onToggleFamily: toggleWatchFamily }}
        empty="Nothing: only the root's own modifiers are tracked."
      />

      <details className="paste">
        <summary>Paste an item</summary>
        <textarea
          rows={10}
          value={paste}
          onChange={(event) => setPaste(event.target.value)}
          placeholder="Ctrl+C an item in game, then paste it here"
        />
        <div className="row-actions">
          <button type="button" onClick={applyPaste}>
            Use it
          </button>
          <select
            value=""
            onChange={(event) => {
              const example = examples.find((e) => e.id === event.target.value)
              if (example) setPaste(example.text)
            }}
          >
            <option value="">Examples…</option>
            {examples.map((example) => (
              <option key={example.id} value={example.id}>
                {example.label}
              </option>
            ))}
          </select>
        </div>
        {pasteError && <p className="error-inline">{pasteError}</p>}
      </details>
    </section>
  )
}
