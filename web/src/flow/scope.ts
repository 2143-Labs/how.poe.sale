/** One flowchart page: which bases it offers and which methods it walks back through. */
export interface Scope {
  id: 'poe1' | 'simple'
  title: string
  /** The one base the page works on, or `null` for every base. */
  base: string | null
  methods: string[]
  storageKey: string
}

/**
 * The methods the flowchart models. "Prefixes Cannot Be Changed" is a suffix craft and
 * "Suffixes Cannot Be Changed" a prefix craft.
 */
export const METHODS = [
  'currency/exalted',
  'currency/annulment',
  'bench/add/StrMasterItemGenerationCannotChangePrefixes',
  'bench/add/DexMasterItemGenerationCannotChangeSuffixes',
  'essence/essence-of-horror',
]

export const SCOPES: Record<Scope['id'], Scope> = {
  poe1: {
    id: 'poe1',
    title: 'Path of Exile 1',
    base: null,
    methods: METHODS,
    storageKey: 'how.poe.sale:flow:poe1',
  },
  simple: {
    id: 'simple',
    title: 'Path of Exile 1 simple',
    base: 'Archdemon Crown',
    methods: METHODS,
    storageKey: 'how.poe.sale:flow:simple',
  },
}

/** Essence of Horror's modifier on a helmet: a suffix. */
export const HORROR_HELMET = 'SocketedGemsDealMoreElementalDamageEssence1'
