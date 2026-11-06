import type { UnmetView } from '../types'

/** The methods that do not apply, grouped by reason, most common first. */
export function UnmetList({ unmet }: { unmet: UnmetView[] }) {
  const byReason = new Map<string, string[]>()
  for (const u of unmet) byReason.set(u.reason, [...(byReason.get(u.reason) ?? []), u.name])
  const reasons = [...byReason].sort((a, b) => b[1].length - a[1].length || a[0].localeCompare(b[0]))
  return (
    <section className="panel">
      <details>
        <summary>Does not apply: {unmet.length} methods</summary>
        <table>
          <tbody>
            {reasons.map(([reason, names]) => (
              <tr key={reason}>
                <td className="num">{names.length}</td>
                <td>{reason}</td>
                <td className="hint">
                  {names.slice(0, 5).join(', ')}
                  {names.length > 5 && ` and ${names.length - 5} more`}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </details>
    </section>
  )
}
