import { useEffect, useMemo, useState } from 'react'
import { useCraft } from './context'
import { DiagramView } from './DiagramView'
import { chaos, count, percent } from './format'
import type { SpendView } from './types'

/** A spend as one weighted number, the orbs behind it, and the counts with no price. */
function Spend({ spend, unit }: { spend: SpendView; unit: string }) {
  return (
    <div>
      <p className="target-summary">
        {chaos(spend.value)} ± {chaos(spend.sd)} {unit}
      </p>
      <table>
        <thead>
          <tr>
            <th>Orb</th>
            <th className="num">Mean</th>
            <th className="num">sd</th>
            <th className="num">Price</th>
          </tr>
        </thead>
        <tbody>
          {spend.orbs.map((orb) => (
            <tr key={orb.id}>
              <td>{orb.name}</td>
              <td className="num">{count(orb.mean)}</td>
              <td className="num">{count(orb.sd)}</td>
              <td className="num">{orb.price === null ? '—' : chaos(orb.price)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {spend.unweighted.length > 0 && (
        <ul className="warnings">
          {spend.unweighted.map((item) => (
            <li key={item.id}>
              {item.name}: {count(item.count)}
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

/** The last run: what it costs, step by step, with each step's diagram and every other move priced. */
export function ResultPanel() {
  const result = useCraft((state) => state.result)
  const error = useCraft((state) => state.error)
  const busy = useCraft((state) => state.busy)
  const catalog = useCraft((state) => state.catalog)
  const [step, setStep] = useState(0)

  const first = result?.steps[0]?.step ?? 0
  useEffect(() => {
    setStep(first)
  }, [first])

  const names = useMemo(() => new Map(catalog.currencies.map((currency) => [currency.id, currency.name])), [catalog])

  if (error) {
    return (
      <aside className="panel">
        <div className="panel-header">
          <h2>Result</h2>
        </div>
        <div className="banner banner-error">{error}</div>
      </aside>
    )
  }

  if (!result) {
    return (
      <aside className="panel">
        <div className="panel-header">
          <h2>Result</h2>
          {busy && <span className="busy">Computing…</span>}
        </div>
        <p className="hint">Pick a craft, its goal and its prices to see what it costs per finished item.</p>
      </aside>
    )
  }

  const others = [...result.others].sort(
    (a, b) => b.visits * (b.plannedCost - b.otherCost) - a.visits * (a.plannedCost - a.otherCost),
  )
  const diagram = result.diagrams.find((entry) => entry.step === step)
  const skipped =
    result.checked.skipped > 0
      ? ` ${result.checked.skipped} other moves could not be priced: the craft has no rule for where they land.`
      : ''

  return (
    <aside className="panel">
      <div className="panel-header">
        <h2>{result.craft}</h2>
        {busy && <span className="busy">Computing…</span>}
      </div>

      {result.warnings.length > 0 && (
        <div className="banner banner-notice">
          <ul className="warnings">
            {result.warnings.map((warning) => (
              <li key={warning}>{warning}</li>
            ))}
          </ul>
        </div>
      )}

      <h3>Total</h3>
      <Spend spend={result.total} unit={result.unit} />

      <h3>Outputs</h3>
      <table>
        <thead>
          <tr>
            <th>Exit</th>
            <th>Step</th>
            <th className="num">Share</th>
          </tr>
        </thead>
        <tbody>
          {result.outputs.map((output) => (
            <tr key={`${output.step}:${output.exit}`}>
              <td>{output.exit}</td>
              <td>step {output.step}</td>
              <td className="num">{percent(output.share)}</td>
            </tr>
          ))}
        </tbody>
      </table>

      <h3>Steps</h3>
      <table className="steps-table">
        <thead>
          <tr>
            <th>Step</th>
            <th>Block</th>
            <th>Method</th>
            <th>Takes</th>
            <th className="num">Chance</th>
            <th className="num">Tries</th>
            <th className="num">One run</th>
            <th className="num">Reach</th>
          </tr>
        </thead>
        <tbody>
          {result.steps.map((entry) => (
            <tr key={entry.step}>
              <td>{entry.step}</td>
              <td>
                <code>{entry.block}</code>
              </td>
              <td>{entry.name}</td>
              <td>{entry.takes.join(', ')}</td>
              <td className="num">{percent(entry.chance)}</td>
              <td className="num">{entry.tries === null ? '—' : count(entry.tries)}</td>
              <td className="num">{chaos(entry.run.value)}</td>
              <td className="num">{chaos(entry.reach.value)}</td>
            </tr>
          ))}
        </tbody>
      </table>

      <h3>Diagram</h3>
      <label className="inline-field">
        Step
        <select value={step} onChange={(event) => setStep(Number(event.target.value))}>
          {result.steps.map((entry) => (
            <option key={entry.step} value={entry.step}>
              {entry.step}: {entry.name} on {entry.block}
            </option>
          ))}
        </select>
      </label>
      {diagram && <DiagramView diagram={diagram} />}

      <h3>Other moves</h3>
      {others.length === 0 ? (
        <p className="hint">No other move was priced at a place this craft passes through.</p>
      ) : (
        <table>
          <thead>
            <tr>
              <th>Step</th>
              <th>Item</th>
              <th>Planned</th>
              <th className="num">Cost</th>
              <th>Other</th>
              <th className="num">Cost</th>
              <th className="num">Visits</th>
            </tr>
          </thead>
          <tbody>
            {others.map((other, index) => (
              <tr key={`${other.step}:${other.item}:${index}`}>
                <td>{other.step}</td>
                <td>{other.item}</td>
                <td>{names.get(other.planned) ?? other.planned}</td>
                <td className="num">{chaos(other.plannedCost)}</td>
                <td>
                  {names.get(other.other) ?? other.other}
                  {other.better && <span className="badge badge-expanded">cheaper</span>}
                </td>
                <td className="num">{chaos(other.otherCost)}</td>
                <td className="num">{count(other.visits)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      <p className="hint">
        {result.checked.moves} moves at {result.checked.places} places checked.{skipped}
      </p>
    </aside>
  )
}
