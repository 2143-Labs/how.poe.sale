import { useEffect, useMemo, useState } from 'react'
import type { ModOption, ModSpec } from '../types'
import { errorText, useFlow, useFlowState } from './context'

/** The most rows a search shows. */
const ROWS = 200

/** The tracked modifiers: the ones checked, and a search over every modifier the base can name. */
export function ModPicker() {
  const { client, store } = useFlow()
  const spec = useFlowState((state) => state.spec)
  const mods = useFlowState((state) => state.mods)
  const [query, setQuery] = useState('')
  const influences = spec.influences.join(',')

  useEffect(() => {
    let cancelled = false
    const timer = setTimeout(() => {
      client
        .mods(spec.base, spec.itemLevel, influences ? influences.split(',') : [])
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
  }, [client, store, spec.base, spec.itemLevel, influences])

  const byId = useMemo(() => new Map(mods.map((m) => [m.id, m])), [mods])
  const checked = new Set([...spec.prefixes, ...spec.suffixes].map((m) => m.id))

  const rows = useMemo(() => {
    const words = query.trim().toLowerCase()
    if (words.length < 2) return []
    return mods.filter((m) => m.text.toLowerCase().includes(words) || m.id.toLowerCase().includes(words)).slice(0, ROWS)
  }, [mods, query])

  function toggle(mod: ModOption, on: boolean) {
    const current = store.getState().spec
    const key = mod.side === 'prefix' ? 'prefixes' : 'suffixes'
    const rest = current[key].filter((m) => m.id !== mod.id)
    store.getState().setSpec({ ...current, [key]: on ? [...rest, { id: mod.id }] : rest })
  }

  function remove(id: string) {
    const current = store.getState().spec
    store.getState().setSpec({
      ...current,
      prefixes: current.prefixes.filter((m) => m.id !== id),
      suffixes: current.suffixes.filter((m) => m.id !== id),
    })
  }

  const tracked = (side: string, list: ModSpec[]) =>
    list.map((m) => (
      <li key={m.id} className="picker-row">
        <span className={`side side-${side}`}>{side}</span>
        <span className="picker-text">{byId.get(m.id)?.text ?? m.id}</span>
        <button type="button" onClick={() => remove(m.id)} aria-label="remove">
          ×
        </button>
      </li>
    ))

  return (
    <div className="picker">
      <h3>Tracked modifiers</h3>
      {spec.prefixes.length + spec.suffixes.length === 0 ? (
        <p className="hint">None: every modifier is an "any" slot.</p>
      ) : (
        <ul className="picker-list">
          {tracked('prefix', spec.prefixes)}
          {tracked('suffix', spec.suffixes)}
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
                  checked={checked.has(mod.id)}
                  onChange={(event) => toggle(mod, event.target.checked)}
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
