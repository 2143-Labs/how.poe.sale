import { useState } from 'react'
import { chance } from './format'
import { useApp, useAppState } from './context'

/** One currency applied to the setup's item, and every outcome it can give. */
export function ApplyPanel() {
  const { store } = useApp()
  const start = useAppState((state) => state.start)
  const applied = useAppState((state) => state.applied)
  const applying = useAppState((state) => state.applying)
  const [picked, setPicked] = useState('')
  if (!start) return null

  const currency = picked || start.currencies[0]?.id || ''

  return (
    <section className="panel">
      <div className="panel-header">
        <h2>Apply a currency</h2>
        <span className="hint">one step from the setup's item, with every outcome and its exact chance</span>
      </div>

      <div className="row-actions">
        <select value={currency} onChange={(event) => setPicked(event.target.value)}>
          {start.currencies.map((entry) => (
            <option key={entry.id} value={entry.id}>
              {entry.name}
            </option>
          ))}
        </select>
        <button
          type="button"
          onClick={() => store.getState().applyCurrency(currency)}
          disabled={applying || currency === ''}
        >
          Apply
        </button>
      </div>

      {applied && (
        <>
          <p className="hint">
            {applied.currency.name}: {applied.outcomes.length} outcomes shown
          </p>
          <table>
            <thead>
              <tr>
                <th className="num">chance</th>
                <th>item</th>
                <th>added</th>
              </tr>
            </thead>
            <tbody>
              {applied.outcomes.map((outcome, index) => (
                <tr key={`${index} ${outcome.text}`}>
                  <td className="num">{chance(outcome.chance)}</td>
                  <td className="item-text">{outcome.text}</td>
                  <td>{outcome.added ?? ''}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {applied.more > 0 && (
            <p className="hint">
              {applied.more} more outcomes, {chance(applied.rest)} together
            </p>
          )}
        </>
      )}
    </section>
  )
}
