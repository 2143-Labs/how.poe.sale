import { useMemo, useState } from 'react'
import { useApp, useAppState } from './context'
import type { ModOptionView, SelectorSpec, Side } from './types'

/** The most rows a search shows. */
const ROWS = 200

function sideOf(option: ModOptionView): Side {
  return option.side === 'suffix' ? 'suffix' : 'prefix'
}

/** The goal's modifiers: a search over everything the base can name, each row desired or good. */
export function ModPicker() {
  const { store } = useApp()
  const mods = useAppState((state) => state.mods)
  const desired = useAppState((state) => state.setup.goal.desired)
  const good = useAppState((state) => state.setup.goal.good)
  const [query, setQuery] = useState('')

  const byId = useMemo(() => new Map(mods.map((mod) => [mod.id, mod])), [mods])
  const gradeOf = (id: string) =>
    desired.some((slot) => slot.id === id) ? 'desired' : good.some((slot) => slot.id === id) ? 'good' : null
  const orBetterOf = (id: string) => [...desired, ...good].find((slot) => slot.id === id)?.orBetter ?? false

  const rows = useMemo(() => {
    const words = query.trim().toLowerCase()
    if (words.length < 2) return []
    return mods.filter((mod) => mod.text.toLowerCase().includes(words) || mod.id.toLowerCase().includes(words)).slice(0, ROWS)
  }, [mods, query])

  function chosenRow(slot: SelectorSpec, grade: 'desired' | 'good') {
    const known = byId.get(slot.id)
    return (
      <li key={`${grade} ${slot.id}`} className="picker-row">
        <span className={`side side-${slot.side}`}>{slot.side}</span>
        <span className="badge">{grade}</span>
        <span className="picker-text">{known?.text ?? slot.id}</span>
        <label className="check">
          <input
            type="checkbox"
            checked={slot.orBetter}
            onChange={(event) => store.getState().setOrBetter(slot.id, event.target.checked)}
          />
          or better
        </label>
        <button type="button" onClick={() => store.getState().setMod(slot.side, slot.id, null)} aria-label="remove">
          ×
        </button>
      </li>
    )
  }

  return (
    <div className="picker">
      <h3>Goal modifiers</h3>
      {desired.length + good.length === 0 ? (
        <p className="hint">No goal yet: every modifier is acceptable, so the loop only resets rares.</p>
      ) : (
        <ul className="picker-list picker-chosen">
          {desired.map((slot) => chosenRow(slot, 'desired'))}
          {good.map((slot) => chosenRow(slot, 'good'))}
        </ul>
      )}
      <input
        type="search"
        className="picker-search"
        placeholder={`Search ${mods.length} modifiers`}
        value={query}
        onChange={(event) => setQuery(event.target.value)}
      />
      {rows.length > 0 && (
        <ul className="picker-list picker-results">
          {rows.map((mod) => {
            const grade = gradeOf(mod.id)
            const side = sideOf(mod)
            return (
              <li key={mod.id} className="picker-row">
                <span className={`side side-${side}`}>{side}</span>
                <span className="picker-text">{mod.text}</span>
                <span className="picker-level">lvl {mod.level}</span>
                {mod.source !== 'natural' && <span className="badge">{mod.source}</span>}
                {mod.influence && <span className="badge">{mod.influence}</span>}
                <label className="check">
                  <input
                    type="checkbox"
                    checked={grade === 'desired'}
                    onChange={(event) => store.getState().setMod(side, mod.id, event.target.checked ? 'desired' : null)}
                  />
                  desired
                </label>
                <label className="check">
                  <input
                    type="checkbox"
                    checked={grade === 'good'}
                    onChange={(event) => store.getState().setMod(side, mod.id, event.target.checked ? 'good' : null)}
                  />
                  good
                </label>
                {grade && (
                  <label className="check">
                    <input
                      type="checkbox"
                      checked={orBetterOf(mod.id)}
                      onChange={(event) => store.getState().setOrBetter(mod.id, event.target.checked)}
                    />
                    or better
                  </label>
                )}
              </li>
            )
          })}
        </ul>
      )}
    </div>
  )
}
