import { fixed } from '../steps'
import type { SourcesView } from '../types'

/** The inputs one step short of the item, grouped by how the pattern reads them. */
export function SourcesPanel({ sources }: { sources: SourcesView }) {
  return (
    <section className="panel">
      <header className="panel-header">
        <h2>Could have come from</h2>
      </header>
      {sources.groups.length === 0 ? (
        <div className="hint">None found.</div>
      ) : (
        <ul className="sources">
          {sources.groups.slice(0, 10).map((group, i) => (
            <li key={i}>
              {group.state} — best {fixed(group.best, 4)}
              <div className="step-does">
                {group.sources.slice(0, 3).map((source, j) => (
                  <span key={j}>
                    {j > 0 && ', '}
                    {source.name} (<code>{source.method}</code>) at {fixed(source.p, 4)}
                    {source.note && ` (${source.note})`}
                  </span>
                ))}
                {group.sources.length > 3 && ` and ${group.sources.length - 3} more`}
              </div>
              <details>
                <summary>Example input</summary>
                <pre className="item-text">{group.example}</pre>
              </details>
            </li>
          ))}
        </ul>
      )}
      {sources.skipped.length > 0 && (
        <details>
          <summary>{sources.skipped.length} methods cannot be walked backwards</summary>
          <ul className="warnings">
            {sources.skipped.map((s) => (
              <li key={s.method}>
                {s.name}: {s.reason}
              </li>
            ))}
          </ul>
        </details>
      )}
    </section>
  )
}
