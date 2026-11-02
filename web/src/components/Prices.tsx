import { useMemo } from 'react'
import { DEFAULT_PRICES, DEFAULT_PRICES_DATE } from '../defaultPrices'
import { useAppStore } from '../store'

/** Prices with no poe.ninja exchange listing: typed by the user, empty by default. */
export const TYPED_PRICE_KEYS = ['craicic-croaker', 'recombiner'] as const

function label(key: string): string {
  return key.replace(/-/g, ' ')
}

export function Prices() {
  const prices = useAppStore((state) => state.prices)
  const processes = useAppStore((state) => state.processes)
  const solution = useAppStore((state) => state.solution)
  const setPrice = useAppStore((state) => state.setPrice)
  const resetPrices = useAppStore((state) => state.resetPrices)

  const keys = useMemo(
    () =>
      Array.from(new Set([...Object.keys(DEFAULT_PRICES), ...TYPED_PRICE_KEYS, ...Object.keys(prices)])).sort(),
    [prices],
  )

  const needed = useMemo(() => {
    const set = new Set<string>()
    for (const process of processes) {
      if ('recombine' in process.kind) set.add('recombiner')
    }
    for (const process of solution?.market.processes ?? []) {
      for (const step of process.steps) for (const [key] of step.cost) set.add(key)
      for (const key of Object.keys(process.spend)) set.add(key)
    }
    return set
  }, [processes, solution])

  return (
    <section className="panel">
      <header className="panel-header">
        <h2>Prices</h2>
        <span className="hint">defaults from {DEFAULT_PRICES_DATE}</span>
        <button type="button" onClick={resetPrices}>
          reset to defaults
        </button>
      </header>
      <table className="prices">
        <thead>
          <tr>
            <th>Item</th>
            <th>chaos</th>
          </tr>
        </thead>
        <tbody>
          {keys.map((key) => (
            <tr key={key}>
              <td>
                {label(key)}
                {needed.has(key) && <span className="badge badge-unreachable">needed</span>}
              </td>
              <td>
                <input
                  type="number"
                  min="0"
                  step="any"
                  value={prices[key] ?? ''}
                  onChange={(event) =>
                    setPrice(key, event.target.value === '' ? null : Number(event.target.value))
                  }
                />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  )
}
