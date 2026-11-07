/** What every number on the page assumes, said once. */
export function Assumptions() {
  return (
    <p className="assumptions">
      Chances assume every added modifier picks its side first: prefix or suffix, evenly among the sides with room, and
      then a modifier on that side by weight (claims <code>add.side-first</code> and <code>reforge.side-first</code>).
      The site owner states this rule; no published source confirms it. Reforges with tracked modifiers are simulated
      and marked ≈. Items reached backward have unknown modifiers; pushing them forward assumes those rolled naturally.
    </p>
  )
}
