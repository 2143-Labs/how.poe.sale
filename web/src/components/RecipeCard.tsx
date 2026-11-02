import { processInputs } from '../market'
import { formatChaos, formatDuration, formatProbability } from '../solution'
import type { ExitJson, GoodJson, ProcessProposalJson, ProcessValueJson } from '../types'

interface RecipeCardProps {
  /** an evaluated process of the market solution */
  process?: ProcessValueJson | null
  /** a proposed process, not yet added */
  proposal?: ProcessProposalJson | null
  goods: GoodJson[]
  onUse?: (proposal: ProcessProposalJson) => void
  onHide?: () => void
  /** clone a good's target into a new, unpriced brick good */
  onMakeBrick?: (fromGoodId: string, name: string) => void
}

function costText(cost: [string, number][]): string {
  if (cost.length === 0) return '—'
  return cost.map(([key, qty]) => `${qty}× ${key.replace(/-/g, ' ')}`).join(', ')
}

function ExitTable({
  title,
  rows,
  output,
  goods,
  onMakeBrick,
}: {
  title: string
  rows: ExitJson[]
  output: string | null
  goods: GoodJson[]
  onMakeBrick?: (fromGoodId: string, name: string) => void
}) {
  if (rows.length === 0) return null
  return (
    <>
      <h3>{title}</h3>
      <table className="exits">
        <thead>
          <tr>
            <th>outcome</th>
            <th>p</th>
            <th>good</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row, index) => {
            const good = row.good ? goods.find((candidate) => candidate.id === row.good) : undefined
            const isBrick = row.good !== output
            return (
              <tr key={`${row.good ?? 'none'}|${index}`}>
                <td>{row.label}</td>
                <td>{formatProbability(row.p)}</td>
                <td>
                  {row.good == null ? (
                    <span className="hint">brick: unpriced</span>
                  ) : (
                    <>
                      {good?.name ?? row.good}
                      {good?.sell != null ? ` · sells ${formatChaos(good.sell)}` : ''}
                      {row.good === output ? <span className="badge badge-expanded">output</span> : null}
                    </>
                  )}
                </td>
                <td>
                  {isBrick && onMakeBrick && output && (
                    <button type="button" onClick={() => onMakeBrick(output, row.label)}>
                      Make brick a good
                    </button>
                  )}
                </td>
              </tr>
            )
          })}
        </tbody>
      </table>
    </>
  )
}

/**
 * One process: what it needs, the numbered steps with their change lists, the
 * outcome tables, and the cost statistics. Used both for a proposal (with
 * **Use**) and for a process already in the market.
 */
export function RecipeCard({ process, proposal, goods, onUse, onHide, onMakeBrick }: RecipeCardProps) {
  const source = process ?? null
  const kind = proposal
    ? 'recombine' in proposal.process.kind
      ? 'recombine'
      : 'craft'
    : (source?.kind ?? 'craft')
  const inputs = proposal ? processInputs(proposal.process) : (source?.inputs ?? [])
  const name = source?.name ?? proposal?.name ?? (kind === 'recombine' ? 'Recombine' : 'Craft')
  const status = source?.status ?? proposal?.status ?? 'NeedsInput'
  const cost = source?.cost ?? proposal?.cost ?? null
  const pSuccess = source?.pSuccess ?? proposal?.pSuccess ?? null
  const output = source?.output ?? proposal?.process.output ?? null
  const steps = source?.steps ?? []
  const core = source?.core ?? []
  const exits = source?.exits ?? []
  const error = source?.error ?? proposal?.reason ?? null

  return (
    <section className="panel recipe-card">
      <header className="panel-header">
        <h2>{name}</h2>
        <span className={`badge status-${status.toLowerCase()}`}>{status}</span>
        {pSuccess != null && <span className="hint">p {formatProbability(pSuccess)}</span>}
      </header>

      <h3>Needs</h3>
      {inputs.length === 0 ? (
        <p className="hint">No input good: the craft starts from the output item itself.</p>
      ) : (
        <ul className="chips">
          {inputs.map((id) => {
            const good = goods.find((candidate) => candidate.id === id)
            return (
              <li key={id} className={`chip ${good?.buy == null ? 'chip-blocked' : 'chip-ok'}`}>
                {good?.name ?? id}
                {good?.buy == null ? ' · no price' : ` · buy ${formatChaos(good.buy)}`}
              </li>
            )
          })}
        </ul>
      )}

      {steps.length > 0 && (
        <>
          <h3>Steps</h3>
          <ol className="steps">
            {steps.map((step, index) => (
              <li key={`${step.label}|${index}`}>
                <div className="step-head">
                  <span>
                    {index + 1}. {step.label}
                  </span>
                  <span className="hint">{costText(step.cost)}</span>
                </div>
                {step.changes.length > 0 && (
                  <ul className="changes">
                    {step.changes.map((change) => (
                      <li key={change}>{change}</li>
                    ))}
                  </ul>
                )}
              </li>
            ))}
          </ol>
        </>
      )}

      <ExitTable title="Core outcomes" rows={core} output={output} goods={goods} onMakeBrick={onMakeBrick} />
      <ExitTable title="Exits" rows={exits} output={output} goods={goods} onMakeBrick={onMakeBrick} />

      <dl className="stats-grid">
        <dt>Total cost</dt>
        <dd>{cost == null ? '—' : `${formatChaos(cost)} chaos`}</dd>
        <dt>SD</dt>
        <dd>{formatChaos(source?.sd)}</dd>
        <dt>Time</dt>
        <dd>{formatDuration(source?.seconds)}</dd>
        <dt>Attempts</dt>
        <dd>{source?.attempts == null ? '—' : formatChaos(source.attempts)}</dd>
        {source && Object.keys(source.spend).length > 0 && (
          <>
            <dt>Spend</dt>
            <dd>
              {Object.entries(source.spend)
                .filter(([, qty]) => Math.abs(qty) > 1e-9)
                .map(([key, qty]) => `${qty.toFixed(2)} ${key.replace(/-/g, ' ')}`)
                .join(', ') || '—'}
            </dd>
          </>
        )}
      </dl>

      {error && <p className="error-inline">{error}</p>}

      <div className="row-actions">
        {proposal && onUse && (
          <button type="button" onClick={() => onUse(proposal)}>
            Use
          </button>
        )}
        {onHide && (
          <button type="button" onClick={onHide}>
            Hide
          </button>
        )}
      </div>
    </section>
  )
}
