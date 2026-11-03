import crown from './archdemon-crown.txt?raw'
import staff from './ezomyte-staff.txt?raw'
import amulet from './lapis-amulet.txt?raw'
import simplex from './simplex-amulet.txt?raw'
import bow from './thicket-bow.txt?raw'
import boots from './two-toned-boots.txt?raw'

/** An example item: its tooltip text as the game copies it, without a buy note. */
export interface DemoItem {
  id: string
  label: string
  text: string
}

export const DEMO_ITEMS: DemoItem[] = [
  { id: 'simplex', label: 'Simplex: Strength + damage per Strength', text: simplex },
  { id: 'crown', label: 'Archdemon Crown: socketed gems', text: crown },
  { id: 'bow', label: 'Thicket Bow: triple T1 physical', text: bow },
  { id: 'staff', label: 'Ezomyte Staff: triple T1 physical', text: staff },
  { id: 'amulet', label: 'Lapis Amulet: +1 all Skill Gems', text: amulet },
  { id: 'boots', label: 'Two-Toned Boots: movement, life, resists', text: boots },
]
