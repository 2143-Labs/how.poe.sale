import { useState } from 'react'
import { fixed, groupTies } from '../steps'
import type { StepView } from '../types'

const SHOWN = 10

/** One group of steps, one row per distinct outcome. */
export function StepsTable({ title, note, steps }: { title: string; note: string; steps: StepView[] }) {
  const [all, setAll] = useState(false)
  const rows = groupTies(steps)
  const shown = all ? rows : rows.slice(0, SHOWN)
  return (
    <section className="panel">
      <header className="panel-header">
        <h2>{title}</h2>
        <span className="hint">{note}</span>
      </header>
      {rows.length === 0 ? (
        <div className="hint">None.</div>
      ) : (
        <table className="steps-table">
          <thead>
            <tr>
              <th>#</th>
              <th>Can</th>
              <th>Step</th>
              <th>Cost</th>
              <th className="num">Finish</th>
              <th className="num">Goal</th>
              <th className="num">Better</th>
              <th className="num">Worse</th>
              <th className="num">Blocked</th>
            </tr>
          </thead>
          <tbody>
            {shown.map(({ first, others }, i) => (
              <tr key={first.method}>
                <td>{i + 1}</td>
                <td>{first.stands}</td>
                <td>
                  {first.name} <code>{first.method}</code>
                  <div className="step-does">{first.does}</div>
                  {others.length > 0 && (
                    <details>
                      <summary>and {others.length} more that do exactly the same</summary>
                      {others.map((step) => step.name).join(', ')}
                    </details>
                  )}
                </td>
                <td>{first.cost}</td>
                <td className="num">{fixed(first.finish)}</td>
                <td className="num">{fixed(first.goal)}</td>
                <td className="num">{fixed(first.better)}</td>
                <td className="num">{fixed(first.worse)}</td>
                <td className="num">{fixed(first.blocked)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {rows.length > SHOWN && (
        <div className="row-actions">
          <button type="button" onClick={() => setAll(!all)}>
            {all ? 'Show fewer' : `Show all ${rows.length} outcomes`}
          </button>
        </div>
      )}
    </section>
  )
}
