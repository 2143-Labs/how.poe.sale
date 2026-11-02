import { useMemo } from 'react'
import { DEFAULT_PRICES, DEFAULT_PRICES_DATE } from '../defaultPrices'
import { useAppStore } from '../store'

function label(key: string): string {
  return key.replace(/-/g, ' ')
}

export function Prices() {
  const prices = useAppStore((state) => state.prices)
  const setPrice = useAppStore((state) => state.setPrice)
  const resetPrices = useAppStore((state) => state.resetPrices)
  const keys = useMemo(() => Array.from(new Set([...Object.keys(DEFAULT_PRICES), ...Object.keys(prices)])).sort(), [prices])

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
              <td>{label(key)}</td>
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
