import { useApp, useAppState } from './context'
import { ModPicker } from './ModPicker'
import type { Accept, Rarity } from './types'

const RARITIES: Rarity[] = ['normal', 'magic', 'rare']

const RARITY_LABELS: Record<Rarity, string> = { normal: 'Normal', magic: 'Magic', rare: 'Rare' }

const ACCEPT_LABELS: Record<Accept, string> = {
  'has-desired': 'Has a desired modifier',
  'desired-or-good-both-sides': 'Desired, or good on both sides',
  any: 'Any item',
}

/** The item the loop starts from and the modifiers it aims for. */
export function SetupPanel() {
  const { store } = useApp()
  const start = useAppState((state) => state.start)
  const item = useAppState((state) => state.setup.item)
  const accept = useAppState((state) => state.setup.goal.accept)
  if (!start) return null

  const bases = start.bases.includes(item.base) ? start.bases : [item.base, ...start.bases]

  function toggleInfluence(name: string, on: boolean) {
    const rest = store.getState().setup.item.influences.filter((influence) => influence !== name)
    store.getState().setItem({ influences: on ? [...rest, name] : rest })
  }

  return (
    <section className="panel">
      <div className="panel-header">
        <h2>Setup</h2>
        <span className="hint">the white item the loop starts from, and what it is aiming for</span>
      </div>

      <div className="field">
        <span>Base</span>
        <select value={item.base} onChange={(event) => store.getState().setItem({ base: event.target.value })}>
          {bases.map((base) => (
            <option key={base} value={base}>
              {base}
            </option>
          ))}
        </select>
      </div>

      <div className="field">
        <span>Item level</span>
        <input
          type="number"
          min={1}
          max={100}
          value={item.itemLevel}
          onChange={(event) => {
            const n = Number.parseInt(event.target.value, 10)
            if (!Number.isNaN(n)) store.getState().setItem({ itemLevel: Math.min(100, Math.max(1, n)) })
          }}
        />
      </div>

      <div className="field">
        <span>Rarity</span>
        <select
          value={item.rarity}
          onChange={(event) => store.getState().setItem({ rarity: event.target.value as Rarity })}
        >
          {RARITIES.map((rarity) => (
            <option key={rarity} value={rarity}>
              {RARITY_LABELS[rarity]}
            </option>
          ))}
        </select>
      </div>

      <div className="field influences">
        <span>Influences</span>
        {start.influences.map((name) => (
          <label key={name} className="check">
            <input
              type="checkbox"
              checked={item.influences.includes(name)}
              onChange={(event) => toggleInfluence(name, event.target.checked)}
            />
            {name}
          </label>
        ))}
      </div>

      <div className="field">
        <span>Accept</span>
        <select value={accept} onChange={(event) => store.getState().setAccept(event.target.value as Accept)}>
          {start.accepts.map((name) => (
            <option key={name} value={name}>
              {ACCEPT_LABELS[name as Accept] ?? name}
            </option>
          ))}
        </select>
      </div>

      <ModPicker />
    </section>
  )
}
