import { useEffect, useMemo, useState } from 'react'
import type { ModOption } from '../types'
import { errorText, useFlow, useFlowState } from './context'

/** The most rows a search shows. */
const ROWS = 200

interface ModPickerProps {
  /** What `selected` and `onToggle` mean: the root's slots, or the modifiers watched for. */
  title: string
  /** The modifier ids already chosen, in the order the panel shows them. */
  selected: string[]
  onToggle: (option: ModOption, on: boolean) => void
  onRemove: (id: string) => void
  /** Whether a chosen slot can stand for its family at that tier or higher. */
  family: {
    /** The ids whose slot is a family. */
    ids: string[]
    /** Adds or removes `orBetter` on one id; omitted when the list does not offer families. */
    onToggleFamily?: (id: string) => void
  }
  /** Text shown when nothing is chosen. */
  empty: string
}

/** A search over every modifier the base can name, with the chosen ones listed above it. */
export function ModPicker({ title, selected, onToggle, onRemove, family, empty }: ModPickerProps) {
  const { client, store } = useFlow()
  const root = useFlowState((state) => state.root)
  const mods = useFlowState((state) => state.mods)
  const [query, setQuery] = useState('')
  const influences = root.influences.join(',')

  useEffect(() => {
    let cancelled = false
    const timer = setTimeout(() => {
      client
        .mods(root.base, root.itemLevel, influences ? influences.split(',') : [])
        .then((list) => {
          if (!cancelled) store.getState().setMods(list)
        })
        .catch((reason: unknown) => {
          if (cancelled) return
          store.getState().setMods([])
          store.getState().setError(errorText(reason))
        })
    }, 200)
    return () => {
      cancelled = true
      clearTimeout(timer)
    }
  }, [client, store, root.base, root.itemLevel, influences])

  const byId = useMemo(() => new Map(mods.map((m) => [m.id, m])), [mods])
  const chosen = new Set(selected)

  const rows = useMemo(() => {
    const words = query.trim().toLowerCase()
    if (words.length < 2) return []
    return mods.filter((m) => m.text.toLowerCase().includes(words) || m.id.toLowerCase().includes(words)).slice(0, ROWS)
  }, [mods, query])

  return (
    <div className="picker">
      <h3>{title}</h3>
      {selected.length === 0 ? (
        <p className="hint">{empty}</p>
      ) : (
        <ul className="picker-list">
          {selected.map((id) => {
            const known = byId.get(id)
            const side = known?.side ?? 'prefix'
            return (
              <li key={id} className="picker-row">
                <span className={`side side-${side}`}>{side}</span>
                <span className="picker-text">{known?.text ?? id}</span>
                {family.onToggleFamily && (
                  <label className="check">
                    <input
                      type="checkbox"
                      checked={family.ids.includes(id)}
                      onChange={() => family.onToggleFamily?.(id)}
                    />
                    or better
                  </label>
                )}
                <button type="button" onClick={() => onRemove(id)} aria-label="remove">
                  ×
                </button>
              </li>
            )
          })}
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
          {rows.map((mod) => (
            <li key={mod.id} className="picker-row">
              <label>
                <input
                  type="checkbox"
                  checked={chosen.has(mod.id)}
                  onChange={(event) => onToggle(mod, event.target.checked)}
                />
                <span className="picker-text">{mod.text}</span>
              </label>
              <span className={`side side-${mod.side}`}>{mod.side}</span>
              <span className="picker-level">lvl {mod.level}</span>
              {mod.source !== 'natural' && <span className="badge">{mod.source}</span>}
              {mod.influence && <span className="badge">{mod.influence}</span>}
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
