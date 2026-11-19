import { createContext, useContext, useEffect, useMemo, useState } from 'react'
import {
  Background,
  Controls,
  Handle,
  Position,
  ReactFlow,
  applyEdgeChanges,
  applyNodeChanges,
} from '@xyflow/react'
import type { Connection, Edge, Node, NodeProps, NodeTypes, ReactFlowInstance } from '@xyflow/react'
import { layoutGraph, type LayoutEdgeEnds, type LayoutNodeSize, type LayoutPositions } from '../flow/layout'
import { useCraft } from './context'
import { find, isBuiltin, outputs, wouldCycle, type Library } from './library'
import type { BlockSpec, Catalog, CraftSpec, MethodSpec, MethodView } from './types'

/** The keys that remove the selected blocks and wires. */
const DELETE_KEYS = ['Backspace', 'Delete']

const STARTS = [
  { id: 'normal', label: 'Normal' },
  { id: 'magic', label: 'Magic' },
  { id: 'rare', label: 'Rare' },
]

interface BlockExit {
  name: string
  /** The rarity an item leaves with, or `null` for a nested craft's output. */
  rarity: string | null
}

type StartData = {
  start: string
  readOnly: boolean
}

type BlockData = {
  block: BlockSpec
  method: MethodView | null
  craftName: string | null
  exits: BlockExit[]
  readOnly: boolean
}

type StartNode = Node<StartData, 'start'>
type BlockNode = Node<BlockData, 'block'>
type DoneNode = Node<Record<string, never>, 'done'>
type CraftNode = StartNode | BlockNode | DoneNode

interface Actions {
  setStart: (start: string) => void
  setMethod: (block: BlockSpec, method: MethodSpec) => void
}

const ActionsContext = createContext<Actions | null>(null)

function useActions(): Actions {
  const actions = useContext(ActionsContext)
  if (!actions) throw new Error('a craft node outside its editor')
  return actions
}

/** The start block: the rarity the craft begins at, and the one wire into the graph. */
function StartNode({ data }: NodeProps<StartNode>) {
  const actions = useActions()
  return (
    <div className="solution-node goal">
      <div className="node-title">Start</div>
      <select value={data.start} disabled={data.readOnly} onChange={(event) => actions.setStart(event.target.value)}>
        {STARTS.map((start) => (
          <option key={start.id} value={start.id}>
            {start.label}
          </option>
        ))}
      </select>
      <Handle type="source" position={Position.Bottom} id="start" />
    </div>
  )
}

/** The parameters the block's method reads: a count, a flag, or an essence. */
function Params({ block, method, readOnly }: { block: BlockSpec; method: MethodView; readOnly: boolean }) {
  const catalog = useCraft((state) => state.catalog)
  const actions = useActions()
  const spec = block.method
  if (!spec) return null
  return (
    <div className="node-sides">
      {method.params.map((param) => {
        if (param.name === 'modifiers') {
          return (
            <label key={param.name} className="check">
              modifiers
              <input
                type="number"
                min={param.min ?? 1}
                max={param.max ?? 6}
                value={spec.modifiers ?? param.min ?? 1}
                disabled={readOnly}
                onChange={(event) => actions.setMethod(block, { ...spec, modifiers: Number(event.target.value) })}
              />
            </label>
          )
        }
        if (param.name === 'augment') {
          return (
            <label key={param.name} className="check">
              <input
                type="checkbox"
                checked={spec.augment ?? false}
                disabled={readOnly}
                onChange={(event) => actions.setMethod(block, { ...spec, augment: event.target.checked })}
              />
              augment
            </label>
          )
        }
        if (param.name === 'essence') {
          return (
            <label key={param.name} className="check">
              essence
              <select
                value={spec.essence ?? catalog.essences[0]?.id ?? ''}
                disabled={readOnly}
                onChange={(event) => actions.setMethod(block, { ...spec, essence: event.target.value })}
              >
                {catalog.essences.map((essence) => (
                  <option key={essence.id} value={essence.id}>
                    {essence.label}
                  </option>
                ))}
              </select>
            </label>
          )
        }
        return null
      })}
    </div>
  )
}

/** One block: a method with its controls and exits, or a saved craft with its outputs. */
function BlockNode({ data, selected }: NodeProps<BlockNode>) {
  const { block, method, craftName, exits, readOnly } = data
  return (
    <div className={['solution-node', selected ? 'is-selected' : ''].filter(Boolean).join(' ')}>
      <Handle type="target" position={Position.Top} id="in" />
      <div className="node-title">{method ? method.label : (craftName ?? 'missing craft')}</div>
      <div className="node-meta">
        <span className="badge">{block.id}</span>
        {method && <span className="badge">{method.takes.join(', ')}</span>}
        {method?.needs && <span className="badge">{method.needs}</span>}
      </div>
      {method && <Params block={block} method={method} readOnly={readOnly} />}
      <ul className="chips">
        {exits.map((exit) => (
          <li key={exit.name} className="chip chip-other">
            {exit.rarity ? `${exit.name} (${exit.rarity})` : exit.name}
          </li>
        ))}
      </ul>
      {exits.map((exit, index) => (
        <Handle
          key={exit.name}
          type="source"
          position={Position.Right}
          id={exit.name}
          style={{ top: `${((index + 1) / (exits.length + 1)) * 100}%` }}
        />
      ))}
    </div>
  )
}

/** The finished item: every exit wired to `done` ends here. */
function DoneNode() {
  return (
    <div className="solution-node">
      <Handle type="target" position={Position.Top} id="in" />
      <div className="node-title">Done</div>
    </div>
  )
}

const nodeTypes: NodeTypes = { start: StartNode, block: BlockNode, done: DoneNode }

/** A nested craft's outputs, named the way the engine names them. */
function exitsOf(spec: CraftSpec, library: Library): BlockExit[] {
  try {
    return outputs(spec, library).map((name) => ({ name, rarity: null }))
  } catch {
    return []
  }
}

/** The ways an item leaves a block: its method's exits, or a nested craft's outputs. */
function blockExits(block: BlockSpec, catalog: Catalog, library: Library): BlockExit[] {
  if (block.method) {
    const kind = block.method.kind
    const method = catalog.methods.find((entry) => entry.kind === kind)
    return method ? method.exits.map((exit) => ({ name: exit.name, rarity: exit.rarity })) : []
  }
  if (block.craft) {
    const nested = find(library, 'craft', block.craft)
    return nested ? exitsOf(nested, library) : []
  }
  return []
}

/** The catalog's method a block names, or `null` when the kind is unknown. */
function methodOf(block: BlockSpec, catalog: Catalog): MethodView | null {
  const spec = block.method
  if (!spec) return null
  return catalog.methods.find((method) => method.kind === spec.kind) ?? null
}

/** A new block's method with the defaults the catalog states. */
function defaultMethod(method: MethodView, catalog: Catalog): MethodSpec {
  const spec: MethodSpec = { kind: method.kind, modifiers: null, augment: null, essence: null }
  for (const param of method.params) {
    if (param.name === 'modifiers') spec.modifiers = typeof param.default === 'number' ? param.default : (param.min ?? 1)
    if (param.name === 'augment') spec.augment = typeof param.default === 'boolean' ? param.default : false
    if (param.name === 'essence') spec.essence = typeof param.default === 'string' ? param.default : (catalog.essences[0]?.id ?? null)
  }
  return spec
}

/** The next free `b{n}` id. */
function nextId(blocks: BlockSpec[]): string {
  let highest = 0
  for (const block of blocks) {
    const match = /^b(\d+)$/.exec(block.id)
    if (match) highest = Math.max(highest, Number(match[1]))
  }
  return `b${highest + 1}`
}

/** The graph's nodes and wires, as the layout wants them. */
function layoutOf(spec: CraftSpec, catalog: Catalog, library: Library): { nodes: LayoutNodeSize[]; edges: LayoutEdgeEnds[] } {
  const ids = new Set<string>(['start', 'done', ...spec.blocks.map((block) => block.id)])
  const nodes: LayoutNodeSize[] = [
    { id: 'start', height: 90 },
    ...spec.blocks.map((block) => ({
      id: block.id,
      height: 110 + blockExits(block, catalog, library).length * 22,
    })),
    { id: 'done', height: 70 },
  ]
  const edges: LayoutEdgeEnds[] = []
  if (spec.entry && ids.has(spec.entry)) edges.push({ id: 'entry', source: 'start', target: spec.entry })
  spec.wires.forEach((wire, index) => {
    if (ids.has(wire.from) && ids.has(wire.to)) {
      edges.push({ id: `w${index}`, source: wire.from, target: wire.to })
    }
  })
  return { nodes, edges }
}

/** A craft as a graph: blocks wired exit to block, with the goal and prices it runs under. */
export function CraftEditor() {
  const catalog = useCraft((state) => state.catalog)
  const library = useCraft((state) => state.library)
  const craftId = useCraft((state) => state.selected.craft)
  const updateCraft = useCraft((state) => state.updateCraft)
  const duplicate = useCraft((state) => state.duplicate)
  const select = useCraft((state) => state.select)
  const [positions, setPositions] = useState<LayoutPositions>({})
  const [nodes, setNodes] = useState<CraftNode[]>([])
  const [edges, setEdges] = useState<Edge[]>([])
  const [instance, setInstance] = useState<ReactFlowInstance<CraftNode, Edge> | null>(null)

  const craft = find(library, 'craft', craftId)
  const readOnly = craft ? isBuiltin(craft.id) : true

  // A change to any block, wire or nested output moves the blocks the page has not placed.
  const shape = useMemo(() => {
    if (!craft) return ''
    const nested = craft.blocks.map((block) => {
      const child = block.craft ? find(library, 'craft', block.craft) : undefined
      return child ? [child.entry, child.wires] : null
    })
    return JSON.stringify([
      craft.entry,
      craft.blocks.map((block) => [block.id, block.method, block.craft]),
      craft.wires,
      nested,
    ])
  }, [craft, library])

  useEffect(() => {
    if (!craft) return
    let cancelled = false
    const { nodes: sized, edges: wired } = layoutOf(craft, catalog, library)
    layoutGraph(sized, wired)
      .then((laid) => {
        if (!cancelled) setPositions(laid)
      })
      .catch(() => {
        // Layout is cosmetic: the blocks keep their last positions.
      })
    return () => {
      cancelled = true
    }
    // Re-layout only when the block and wire set changes.
  }, [shape])

  const derivedNodes = useMemo<CraftNode[]>(() => {
    if (!craft) return []
    const start: StartNode = {
      id: 'start',
      type: 'start',
      data: { start: craft.start, readOnly },
      position: positions.start ?? { x: 0, y: 0 },
      deletable: false,
    }
    const done: DoneNode = {
      id: 'done',
      type: 'done',
      data: {},
      position: positions.done ?? { x: 0, y: 600 },
      deletable: false,
    }
    const blocks: BlockNode[] = craft.blocks.map((block, index) => {
      const nested = block.craft ? find(library, 'craft', block.craft) : undefined
      return {
        id: block.id,
        type: 'block',
        data: {
          block,
          method: methodOf(block, catalog),
          craftName: nested?.name ?? null,
          exits: blockExits(block, catalog, library),
          readOnly,
        },
        position: block.position
          ? { x: block.position[0], y: block.position[1] }
          : (positions[block.id] ?? { x: index * 300, y: 160 }),
      }
    })
    return [start, ...blocks, done]
  }, [craft, catalog, library, positions, readOnly])

  useEffect(() => {
    setNodes(derivedNodes)
  }, [derivedNodes])

  const derivedEdges = useMemo<Edge[]>(() => {
    if (!craft) return []
    const edges: Edge[] = []
    if (craft.entry) {
      edges.push({ id: 'entry', source: 'start', sourceHandle: 'start', target: craft.entry, targetHandle: 'in' })
    }
    craft.wires.forEach((wire, index) => {
      edges.push({
        id: `w${index}`,
        source: wire.from,
        sourceHandle: wire.exit,
        target: wire.to === 'done' ? 'done' : wire.to,
        targetHandle: 'in',
      })
    })
    return edges
  }, [craft])

  useEffect(() => {
    setEdges(derivedEdges)
  }, [derivedEdges])

  useEffect(() => {
    if (!instance) return
    const handle = requestAnimationFrame(() => instance.fitView({ padding: 0.2, maxZoom: 1 }))
    return () => cancelAnimationFrame(handle)
  }, [instance, positions])

  const actions = useMemo<Actions | null>(() => {
    if (!craft) return null
    return {
      setStart(start) {
        if (!readOnly) updateCraft({ ...craft, start })
      },
      setMethod(block, method) {
        if (readOnly) return
        updateCraft({
          ...craft,
          blocks: craft.blocks.map((entry) => (entry.id === block.id ? { ...entry, method } : entry)),
        })
      },
    }
  }, [craft, readOnly, updateCraft])

  if (!craft || !actions) return <p className="hint">No craft is selected.</p>

  const spec = craft

  function add(block: BlockSpec) {
    const xs = spec.blocks.map((entry) => entry.position?.[0] ?? 0)
    const maxX = xs.length > 0 ? Math.max(...xs) : 0
    const placed: BlockSpec = { ...block, position: [maxX + 300, 0] }
    updateCraft({ ...spec, blocks: [...spec.blocks, placed] })
  }

  function tidy() {
    const { nodes: sized, edges: wired } = layoutOf(spec, catalog, library)
    layoutGraph(sized, wired)
      .then((laid) => {
        updateCraft({
          ...spec,
          blocks: spec.blocks.map((block): BlockSpec => {
            const at = laid[block.id]
            return at ? { ...block, position: [at.x, at.y] } : block
          }),
        })
      })
      .catch(() => {
        // Leave the blocks where they are.
      })
  }

  function connect(connection: Connection) {
    if (connection.target === 'start') return
    const next: CraftSpec = { ...spec }
    if (connection.source === 'start') {
      if (connection.target === 'done') return
      next.entry = connection.target
      updateCraft(next)
      return
    }
    const exit = connection.sourceHandle
    if (!exit) return
    const to = connection.target === 'done' ? 'done' : connection.target
    next.wires = [
      ...spec.wires.filter((wire) => !(wire.from === connection.source && wire.exit === exit)),
      { from: connection.source, exit, to },
    ]
    updateCraft(next)
  }

  return (
    <div>
      <div className="panel-header">
        <h2>{spec.name}</h2>
        {readOnly && (
          <>
            <span className="badge">built-in</span>
            <button type="button" onClick={() => duplicate('craft', spec.id)}>
              Duplicate to edit
            </button>
          </>
        )}
        <span className="spacer" />
        <label className="inline-field">
          Goal
          <select
            value={spec.goal}
            disabled={readOnly}
            onChange={(event) => {
              updateCraft({ ...spec, goal: event.target.value })
              select('goal', event.target.value)
            }}
          >
            {library.goals.map((goal) => (
              <option key={goal.id} value={goal.id}>
                {goal.name}
              </option>
            ))}
          </select>
        </label>
        <label className="inline-field">
          Prices
          <select
            value={spec.prices}
            disabled={readOnly}
            onChange={(event) => {
              updateCraft({ ...spec, prices: event.target.value })
              select('prices', event.target.value)
            }}
          >
            {library.prices.map((prices) => (
              <option key={prices.id} value={prices.id}>
                {prices.name}
              </option>
            ))}
          </select>
        </label>
        <button type="button" onClick={tidy} disabled={readOnly}>
          Tidy
        </button>
      </div>

      {!readOnly && (
        <div className="row-actions">
          {catalog.methods.map((method) => (
            <button
              key={method.kind}
              type="button"
              onClick={() =>
                add({ id: nextId(spec.blocks), method: defaultMethod(method, catalog), craft: null, position: null })
              }
            >
              {method.label}
            </button>
          ))}
          {library.crafts
            .filter((candidate) => candidate.id !== spec.id && !wouldCycle(spec.id, candidate.id, library))
            .map((candidate) => (
              <button
                key={candidate.id}
                type="button"
                onClick={() => add({ id: nextId(spec.blocks), method: null, craft: candidate.id, position: null })}
              >
                {candidate.name}
              </button>
            ))}
        </div>
      )}

      <div className="craft-canvas">
        <ActionsContext.Provider value={actions}>
          <ReactFlow<CraftNode, Edge>
            nodes={nodes}
            edges={edges}
            nodeTypes={nodeTypes}
            onInit={setInstance}
            onNodesChange={(changes) => setNodes((current) => applyNodeChanges(changes, current))}
            onEdgesChange={(changes) => setEdges((current) => applyEdgeChanges(changes, current))}
            onConnect={connect}
            isValidConnection={(connection) => connection.target !== 'start'}
            onDelete={(params) => {
              const gone = new Set(
                params.nodes.map((node) => node.id).filter((id) => id !== 'start' && id !== 'done'),
              )
              const dropped = new Set(params.edges.map((edge) => edge.id))
              const entry = gone.has(spec.entry) || dropped.has('entry') ? '' : spec.entry
              const wires = spec.wires.filter(
                (wire, index) => !dropped.has(`w${index}`) && !gone.has(wire.from) && !gone.has(wire.to),
              )
              updateCraft({ ...spec, entry, wires, blocks: spec.blocks.filter((block) => !gone.has(block.id)) })
            }}
            onNodeDragStop={(_event, node) => {
              if (node.type !== 'block') return
              const position: [number, number] = [node.position.x, node.position.y]
              updateCraft({
                ...spec,
                blocks: spec.blocks.map((block) => (block.id === node.id ? { ...block, position } : block)),
              })
            }}
            deleteKeyCode={DELETE_KEYS}
            nodesDraggable={!readOnly}
            nodesConnectable={!readOnly}
            elementsSelectable={!readOnly}
            colorMode="dark"
            minZoom={0.2}
            proOptions={{ hideAttribution: true }}
          >
            <Background />
            <Controls showInteractive={false} />
          </ReactFlow>
        </ActionsContext.Provider>
      </div>
      <p className="hint">
        Wire the start to a block, and each exit to the next block or to done. Backspace or Delete removes the selected
        blocks and wires.
      </p>
    </div>
  )
}
