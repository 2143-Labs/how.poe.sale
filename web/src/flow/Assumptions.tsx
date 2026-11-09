/** What every number on the page assumes, said once. */
export function Assumptions() {
  return (
    <p className="assumptions">
      Chances assume every added modifier picks its side first: prefix or suffix, evenly among the sides with room, and
      then a modifier on that side by weight (claims <code>add.side-first</code> and <code>reforge.side-first</code>).
      The site owner states this rule; no published source confirms it. A reforge is exact when no tracked modifier can
      land and it is not a Harvest reforge that guarantees a tag; otherwise it is simulated and marked ≈. "Tries"
      appears only on a step you can use again on its own result with the same odds. A Veiled Chaos Orb's veil
      replaces one of the modifiers its reforge added, each taken as equally likely (claim
      <code>veiled-chaos.replaces-one</code> does not say which). Items reached backward have unknown modifiers; pushing
      them forward assumes those rolled naturally.
    </p>
  )
}
