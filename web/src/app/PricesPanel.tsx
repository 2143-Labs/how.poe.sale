import { useApp, useAppState } from './context'

/** The currency prices the priced loop charges, and the button that solves it. */
export function PricesPanel() {
  const { store } = useApp()
  const start = useAppState((state) => state.start)
  const prices = useAppState((state) => state.setup.prices)
  const busy = useAppState((state) => state.busy)
  if (!start) return null

  const names = new Map<string, string>()
  for (const currency of start.currencies) {
    for (const cost of currency.costs) if (!names.has(cost.key)) names.set(cost.key, cost.name)
  }
  const keys = Object.keys(start.priceDefaults)

  return (
    <section className="panel">
      <div className="panel-header">
        <h2>Prices</h2>
        <span className="hint">chaos per use; clear a price to leave a currency unpriced</span>
      </div>

      <div className="prices">
        {keys.map((key) => (
          <label key={key} className="price-row">
            <span>{names.get(key) ?? key}</span>
            <input
              type="number"
              min={0}
              step="0.001"
              value={prices.prices[key] ?? ''}
              onChange={(event) => {
                const text = event.target.value.trim()
                store.getState().setPrice(key, text === '' ? null : Number.parseFloat(text))
              }}
            />
          </label>
        ))}
      </div>

      <div className="field">
        <span>Trade penalty</span>
        <input
          type="number"
          min={0}
          max={1}
          step="0.01"
          value={prices.tradePenalty}
          onChange={(event) => store.getState().setTradePenalty(Number.parseFloat(event.target.value))}
        />
      </div>

      <div className="row-actions">
        <button type="button" onClick={() => store.getState().priceLoop()} disabled={busy}>
          Price the loop
        </button>
        <span className="hint">the priced graph takes a few seconds</span>
      </div>
    </section>
  )
}
