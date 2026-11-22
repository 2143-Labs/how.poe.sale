/** One game page: its title, the data bundle it loads, and the setup a new page starts from. */
export interface Scope {
  id: 'poe1' | 'simple'
  title: string
  /** The URL of the game's data bundle. */
  bundle: string
  defaultBase: string
  /** The item level a new page opens on: every modifier in both games' data is in reach at 86. */
  defaultItemLevel: number
  storageKey: string
}

export const SCOPES: Record<Scope['id'], Scope> = {
  poe1: {
    id: 'poe1',
    title: 'Path of Exile 1',
    bundle: '/data/bundle.json',
    defaultBase: 'Archdemon Crown',
    defaultItemLevel: 86,
    storageKey: 'how.poe.sale:app:poe1',
  },
  simple: {
    id: 'simple',
    title: 'Simple',
    bundle: '/data/simple.json',
    defaultBase: 'Helmet',
    defaultItemLevel: 86,
    storageKey: 'how.poe.sale:app:simple',
  },
}
