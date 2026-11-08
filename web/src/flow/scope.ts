/** One flowchart page: a game, the data bundle it loads, and the base a new chart starts on. */
export interface Scope {
  id: 'poe1' | 'simple'
  title: string
  /** The URL of the game's data bundle. */
  bundle: string
  defaultBase: string
  storageKey: string
}

export const SCOPES: Record<Scope['id'], Scope> = {
  poe1: {
    id: 'poe1',
    title: 'Path of Exile 1',
    bundle: '/data/bundle.json',
    defaultBase: 'Archdemon Crown',
    storageKey: 'how.poe.sale:flow:poe1',
  },
  simple: {
    id: 'simple',
    title: 'Simple',
    bundle: '/data/simple.json',
    defaultBase: 'Helmet',
    storageKey: 'how.poe.sale:flow:simple-game',
  },
}
