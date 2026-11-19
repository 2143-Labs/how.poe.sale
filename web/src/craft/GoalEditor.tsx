import { useEffect, useMemo, useState } from 'react'
import { useCraft } from './context'
import { find, isBuiltin } from './library'
import type { ModView } from './types'

interface FloorsProps {
  title: string
  floors: string[]
  /** The most floors this side may hold: the base's rare cap. */
  cap: number
  grades: string[]
  disabled: boolean
  onChange: (floors: string[]) => void
}

/** One side's grade floors: a chip per floor, and an Add that stops at the base's cap. */
function Floors({ title, floors, cap, grades, disabled, onChange }: FloorsProps) {
  return (
    <div className="field">
      <span>{title}</span>
      <div className="row-actions">
        {floors.map((grade, index) => (
          <span key={index} className="chip">
            <select
              value={grade}
              disabled={disabled}
              onChange={(event) => onChange(floors.map((entry, at) => (at === index ? event.target.value : entry)))}
            >
              {grades.map((name) => (
                <option key={name} value={name}>
                  {name}
                </option>
              ))}
            </select>
            <button
              type="button"
              disabled={disabled}
              aria-label="remove"
              onClick={() => onChange(floors.filter((_, at) => at !== index))}
            >
              ×
            </button>
          </span>
        ))}
        <button type="button" disabled={disabled || floors.length >= cap} onClick={() => onChange([...floors, 'any'])}>
          Add
        </button>
      </div>
    </div>
  )
}

/** The goal a run is built for: its base, influence, grade floors and the modifiers each grade is made of. */
export function GoalEditor() {
  const catalog = useCraft((state) => state.catalog)
  const library = useCraft((state) => state.library)
  const goalId = useCraft((state) => state.selected.goal)
  const mods = useCraft((state) => state.mods)
  const loadMods = useCraft((state) => state.loadMods)
  const updateGoal = useCraft((state) => state.updateGoal)
  const duplicate = useCraft((state) => state.duplicate)
  const [query, setQuery] = useState('')

  const goal = find(library, 'goal', goalId)
  const baseKey = goal?.base ?? ''
  const base = catalog.bases.find((entry) => entry.key === baseKey)
  const list = mods[baseKey] ?? []
  const readOnly = goal ? isBuiltin(goal.id) : true

  useEffect(() => {
    if (baseKey) loadMods(baseKey)
  }, [baseKey, loadMods])

  const rows = useMemo(() => {
    const words = query.trim().toLowerCase()
    if (words === '') return list
    return list.filter(
      (mod) =>
        mod.text.toLowerCase().includes(words) ||
        mod.name.toLowerCase().includes(words) ||
        mod.group.toLowerCase().includes(words),
    )
  }, [list, query])

  if (!goal || !base) return <p className="hint">No goal is selected.</p>

  function toggle(mod: ModView, kind: 'desired' | 'good', on: boolean) {
    if (!goal) return
    const desired = goal.desired.filter((id) => id !== mod.id)
    const good = goal.good.filter((id) => id !== mod.id)
    if (on) {
      if (kind === 'desired') desired.push(mod.id)
      else good.push(mod.id)
    }
    updateGoal({ ...goal, desired, good })
  }

  return (
    <div>
      <div className="panel-header">
        <h2>{goal.name}</h2>
        {readOnly && (
          <>
            <span className="badge">built-in</span>
            <button type="button" onClick={() => duplicate('goal', goal.id)}>
              Duplicate to edit
            </button>
          </>
        )}
      </div>

      <label className="field">
        <span>Base</span>
        <select
          value={goal.base}
          disabled={readOnly}
          onChange={(event) => updateGoal({ ...goal, base: event.target.value, desired: [], good: [] })}
        >
          {catalog.bases.map((entry) => (
            <option key={entry.key} value={entry.key}>
              {entry.label}
            </option>
          ))}
        </select>
      </label>

      <label className="field">
        <span>Influence</span>
        <select
          value={goal.influence}
          disabled={readOnly}
          onChange={(event) => updateGoal({ ...goal, influence: event.target.value })}
        >
          <option value="none">none</option>
          {base.influences.map((influence) => (
            <option key={influence} value={influence}>
              {influence}
            </option>
          ))}
        </select>
      </label>

      <Floors
        title="Prefix floors"
        floors={goal.prefixes}
        cap={base.caps.rare}
        grades={catalog.grades}
        disabled={readOnly}
        onChange={(floors) => updateGoal({ ...goal, prefixes: floors })}
      />
      <Floors
        title="Suffix floors"
        floors={goal.suffixes}
        cap={base.caps.rare}
        grades={catalog.grades}
        disabled={readOnly}
        onChange={(floors) => updateGoal({ ...goal, suffixes: floors })}
      />

      <h3>Modifiers</h3>
      <input
        type="search"
        className="picker-search"
        placeholder={`Search ${list.length} modifiers`}
        value={query}
        onChange={(event) => setQuery(event.target.value)}
      />
      <table>
        <thead>
          <tr>
            <th>Side</th>
            <th>Name</th>
            <th>Text</th>
            <th className="num">Level</th>
            <th>Group</th>
            <th className="num">Weight</th>
            <th>Desired</th>
            <th>Good</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((mod) => (
            <tr key={mod.id}>
              <td>
                <span className={`side side-${mod.side}`}>{mod.side}</span>
              </td>
              <td>
                {mod.name}
                {mod.essenceOnly && <span className="badge">essence</span>}
              </td>
              <td>{mod.text}</td>
              <td className="num">{mod.level}</td>
              <td>{mod.group}</td>
              <td className="num">
                {goal.influence === 'none' ? mod.weightPlain : (mod.weightInfluenced ?? '—')}
              </td>
              <td>
                <input
                  type="checkbox"
                  checked={goal.desired.includes(mod.id)}
                  disabled={readOnly}
                  onChange={(event) => toggle(mod, 'desired', event.target.checked)}
                />
              </td>
              <td>
                <input
                  type="checkbox"
                  checked={goal.good.includes(mod.id)}
                  disabled={readOnly}
                  onChange={(event) => toggle(mod, 'good', event.target.checked)}
                />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
