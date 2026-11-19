import { useEffect, useState } from 'react'
import { useCraft } from './context'
import { find, isBuiltin } from './library'

/** One price list: the unit the numbers are in, and one weight per orb; an empty box leaves the orb counted. */
export function PriceEditor() {
  const catalog = useCraft((state) => state.catalog)
  const library = useCraft((state) => state.library)
  const priceId = useCraft((state) => state.selected.prices)
  const updatePrices = useCraft((state) => state.updatePrices)
  const duplicate = useCraft((state) => state.duplicate)

  const prices = find(library, 'prices', priceId)
  const id = prices?.id ?? ''
  const [text, setText] = useState<Record<string, string>>(() =>
    Object.fromEntries(Object.entries(prices?.weights ?? {}).map(([key, value]) => [key, String(value)])),
  )
  const readOnly = prices ? isBuiltin(prices.id) : true

  useEffect(() => {
    if (!prices) return
    const next: Record<string, string> = {}
    for (const [key, value] of Object.entries(prices.weights)) next[key] = String(value)
    setText(next)
    // Reset only when a different list is opened.
  }, [id])

  if (!prices) return <p className="hint">No price list is selected.</p>

  const known = new Set(catalog.currencies.map((currency) => currency.id))
  const rows = [
    ...catalog.currencies
      .filter((currency) => currency.priced)
      .map((currency) => ({ id: currency.id, name: currency.name })),
    ...Object.keys(prices.weights)
      .filter((key) => !known.has(key))
      .map((key) => ({ id: key, name: key })),
  ]

  function edit(key: string, value: string) {
    if (!prices) return
    setText((current) => ({ ...current, [key]: value }))
    const weights = { ...prices.weights }
    const weight = Number.parseFloat(value)
    if (value.trim() === '' || !Number.isFinite(weight)) delete weights[key]
    else weights[key] = weight
    updatePrices({ ...prices, weights })
  }

  return (
    <div>
      <div className="panel-header">
        <h2>{prices.name}</h2>
        {readOnly && (
          <>
            <span className="badge">built-in</span>
            <button type="button" onClick={() => duplicate('prices', prices.id)}>
              Duplicate to edit
            </button>
          </>
        )}
      </div>

      <label className="field">
        <span>Unit</span>
        <input
          value={prices.unit}
          disabled={readOnly}
          onChange={(event) => updatePrices({ ...prices, unit: event.target.value })}
        />
      </label>

      <table>
        <thead>
          <tr>
            <th>Currency</th>
            <th>Id</th>
            <th className="num">Price</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.id}>
              <td>{row.name}</td>
              <td className="hint">{row.id}</td>
              <td className="num">
                <input
                  type="number"
                  step="any"
                  value={text[row.id] ?? ''}
                  disabled={readOnly}
                  onChange={(event) => edit(row.id, event.target.value)}
                />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className="hint">An empty box leaves that orb counted rather than priced.</p>
    </div>
  )
}
