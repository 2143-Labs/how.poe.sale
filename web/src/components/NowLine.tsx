import { fixed } from '../steps'
import type { NowView } from '../types'

/** Where the item stands against the goal. */
export function NowLine({ now }: { now: NowView }) {
  return (
    <section className="panel">
      <div>
        Now: {fixed(now.progress)} of the goal · {fixed(now.matches)} chance it is finished · {fixed(now.blocked)}{' '}
        chance a side is full with a slot the pattern wants still empty.
      </div>
      {!now.fits && (
        <div className="error-inline">
          This item does not fit the pattern's item filter (base, item level, rarity, influences, corruption), so no
          step can move it toward the goal.
        </div>
      )}
    </section>
  )
}
