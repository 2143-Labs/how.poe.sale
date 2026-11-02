import data from './defaultPrices.json'

/** Allflame exchange prices in chaos, 2026-11-02. Shared with the Rust tests. */
export const DEFAULT_PRICES: Record<string, number> = data.prices

export const DEFAULT_PRICES_DATE: string = data.date
