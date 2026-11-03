import { formatChaos, formatDivine, formatDuration } from '../solution'
import { useAppStore } from '../store'
import type { Q4 } from '../types'

function QuantileRow({ name, q }: { name: string; q: Q4 | null | undefined }) {
  return (
    <tr>
      <td>{name}</td>
      <td>{q ? formatChaos(q.p50) : '—'}</td>
      <td>{q ? formatChaos(q.p90) : '—'}</td>
      <td>{q ? formatChaos(q.p95) : '—'}</td>
      <td>{q ? formatChaos(q.p99) : '—'}</td>
    </tr>
  )
}

export function Stats() {
  const solution = useAppStore((state) => state.solution)
  const prices = useAppStore((state) => state.prices)
  const stats = solution?.focus?.stats ?? null
  const warnings = [...(solution?.warnings ?? []), ...(solution?.focus?.warnings ?? [])]
  const divine = prices.divine ?? 0
  // Line items the policy never reaches come back as 0 (or float dust); they are not spend.
  const spend = Object.entries(stats?.expectedSpend ?? {}).filter(([, amount]) => Math.abs(amount) > 1e-9)

  return (
    <section className="panel">
      <header className="panel-header">
        <h2>Stats</h2>
      </header>
      {solution === null ? (
        <p className="hint">Waiting for the first solve…</p>
      ) : stats ? (
        <>
          <dl className="stats-grid">
            <dt>Expected cost</dt>
            <dd>
              {formatChaos(stats.expectedCost)} chaos ({formatDivine(stats.expectedCost, divine)})
            </dd>
            <dt>Total SD</dt>
            <dd>{formatChaos(stats.sdCost)} chaos</dd>
            <dt>Expected time</dt>
            <dd>{formatDuration(stats.expectedSeconds)}</dd>
            <dt>Spent buying</dt>
            <dd>{formatChaos(stats.expectedBuyChaos)} chaos</dd>
            <dt>Recovered selling</dt>
            <dd>{formatChaos(stats.expectedSellChaos)} chaos</dd>
          </dl>
          <table className="quantiles">
            <thead>
              <tr>
                <th></th>
                <th>p50</th>
                <th>p90</th>
                <th>p95</th>
                <th>p99</th>
              </tr>
            </thead>
            <tbody>
              <QuantileRow name="cost" q={stats.quantiles?.cost} />
              <QuantileRow name="seconds" q={stats.quantiles?.seconds} />
            </tbody>
          </table>
          <h3>Expected spend</h3>
          <table className="spend">
            <tbody>
              {spend.length === 0 ? (
                <tr>
                  <td>—</td>
                </tr>
              ) : (
                spend.map(([key, amount]) => (
                  <tr key={key}>
                    <td>{key.replace(/-/g, ' ')}</td>
                    <td>{amount.toFixed(2)}</td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </>
      ) : (
        <p className="hint">
          No route reaches the goal under the current policy yet. Add a buy price on the goal (or expand a leaf) and
          solve again.
        </p>
      )}
      {solution?.range && (
        <>
          <p className="hint">
            Expected cost moves between {formatChaos(solution.range.low)} and {formatChaos(solution.range.high)} chaos
            under the approximate rules below.
          </p>
          <table className="readings">
            <tbody>
              {solution.range.readings.map((reading) => (
                <tr key={reading.label}>
                  <td>{reading.label}</td>
                  <td>{reading.value == null ? '—' : formatChaos(reading.value)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
      {solution && solution.approximate.length > 0 && (
        <>
          <h3>Rests on approximate rules</h3>
          <ul className="approximate">
            {solution.approximate.map((claim) => (
              <li key={claim.id}>
                <a href={claim.url} target="_blank" rel="noreferrer">
                  {claim.statement}
                </a>{' '}
                — {claim.reason}
              </li>
            ))}
          </ul>
        </>
      )}
      {warnings.length > 0 && (
        <ul className="warnings">
          {warnings.map((warning) => (
            <li key={warning}>{warning}</li>
          ))}
        </ul>
      )}
    </section>
  )
}
