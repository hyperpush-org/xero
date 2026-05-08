import dagre from '@dagrejs/dagre'
import type { Edge, Node, XYPosition } from '@xyflow/react'

import {
  AGENT_GRAPH_HEADER_NODE_ID,
  AGENT_GRAPH_OUTPUT_NODE_ID,
  type AgentGraphNode,
} from './build-agent-graph'

export interface NodeSize {
  width: number
  height: number
}

export interface LayoutOptions {
  rankdir?: 'LR' | 'TB' | 'RL' | 'BT'
  ranksep?: number
  nodesep?: number
  edgesep?: number
  defaultSize?: NodeSize
}

export interface CategoryLayoutOptions {
  /**
   * Height to use when computing the header's vertical centre line. When the
   * header card is inline-expanded, its actual size grows but the side lanes
   * (tools / database / consumes) and their lane labels are anchored to this
   * value, so they don't drift when the user toggles the header body.
   */
  stableHeaderHeight?: number
}

const DEFAULTS: Required<LayoutOptions> = {
  rankdir: 'LR',
  ranksep: 110,
  nodesep: 36,
  edgesep: 24,
  defaultSize: { width: 280, height: 120 },
}

/**
 * Compass-style category layout.
 *
 *   [consumed]   [prompts row]
 *        \             |
 *         \            v
 *  [consumed] -> [header] -----> [tools — upper-right columns]
 *                       \-----> [database — lower-right columns]
 *                              |
 *                         [output]
 *                              |
 *                       [output sections (3-wide grid)]
 *
 * The header sits in the centre. Prompts stack above it, output sits below
 * with its sections fanning out beneath, and tools/database fan out to the
 * right (tools above the header centre line, database below). Consumed
 * artifacts mirror prompts on the left, flowing into the header.
 *
 * Synthetic `lane-label` nodes are emitted per non-empty category for
 * scannability — they are non-draggable / non-selectable.
 */
export function layoutAgentGraphByCategory(
  nodes: AgentGraphNode[],
  sizes: Map<string, NodeSize>,
  options: CategoryLayoutOptions = {},
): AgentGraphNode[] {
  const VERT_GAP = 90
  const HORIZ_GAP = 110
  const COLUMN_GAP = 48
  const ROW_GAP = 12
  const LANE_LABEL_HEIGHT = 26
  const LANE_LABEL_GAP = 14
  const HEADER_BAND_GAP = 80
  const MAX_TOOLS_PER_COLUMN = 6
  const MAX_DBS_PER_COLUMN = 8
  // Output sections render as a single vertical column under the output node so
  // each output→section edge becomes a clean drop down a shared bus instead of
  // fanning across a multi-column grid (which produced visibly tangled edges
  // when section count was high).
  const SECTION_GRID_COLS = 1
  const SECTION_GRID_GAP = 12

  type CategoryKey =
    | 'prompt'
    | 'tool'
    | 'db-table'
    | 'agent-output'
    | 'output-section'
    | 'consumed-artifact'

  const CATEGORY_LABELS: Record<CategoryKey, string> = {
    prompt: 'Prompts',
    tool: 'Tools',
    'db-table': 'Database',
    'agent-output': 'Output',
    'output-section': 'Output Sections',
    'consumed-artifact': 'Consumes',
  }

  let header: AgentGraphNode | null = null
  const grouped: Record<CategoryKey, AgentGraphNode[]> = {
    prompt: [],
    tool: [],
    'db-table': [],
    'agent-output': [],
    'output-section': [],
    'consumed-artifact': [],
  }
  // Tool category frames + per-frame children. Frames are emitted by
  // build-agent-graph (one per tool.group); tools carry parentId pointing at
  // their frame so React Flow draws them inside it.
  const toolFrames: AgentGraphNode[] = []
  const toolsByFrameId = new Map<string, AgentGraphNode[]>()

  for (const node of nodes) {
    if (node.id === AGENT_GRAPH_HEADER_NODE_ID) {
      header = node
      continue
    }
    if (node.type === 'tool-group-frame') {
      toolFrames.push(node)
      continue
    }
    if (node.type === 'agent-output' || node.id === AGENT_GRAPH_OUTPUT_NODE_ID) {
      grouped['agent-output'].push(node)
    } else if (node.type === 'prompt') {
      grouped.prompt.push(node)
    } else if (node.type === 'tool') {
      grouped.tool.push(node)
      const parentId = (node as Node).parentId
      if (parentId) {
        const arr = toolsByFrameId.get(parentId) ?? []
        arr.push(node)
        toolsByFrameId.set(parentId, arr)
      }
    } else if (node.type === 'db-table') {
      grouped['db-table'].push(node)
    } else if (node.type === 'output-section') {
      grouped['output-section'].push(node)
    } else if (node.type === 'consumed-artifact') {
      grouped['consumed-artifact'].push(node)
    }
  }

  const headerSize =
    sizes.get(AGENT_GRAPH_HEADER_NODE_ID) ?? { width: 300, height: 210 }
  // Side lanes anchor to this height so toggling the header body doesn't drag
  // the DATABASE / TOOLS / CONSUMES lane labels around. Output (which sits
  // directly below the header) still uses headerSize.height so it cleanly
  // clears the expanded body.
  const stableHeaderHeight = options.stableHeaderHeight ?? headerSize.height
  const headerX = 0
  const headerY = 0
  const headerCenterX = headerX + headerSize.width / 2
  const headerCenterY = headerY + stableHeaderHeight / 2

  const placedById = new Map<string, AgentGraphNode>()
  const laneLabelNodes: AgentGraphNode[] = []

  if (header) {
    placedById.set(header.id, { ...header, position: { x: headerX, y: headerY } as XYPosition })
  }

  // 1. Prompts: row above the header, centred on the header's x centre.
  const prompts = grouped.prompt
  if (prompts.length > 0) {
    let maxPromptHeight = 0
    let totalWidth = 0
    for (const p of prompts) {
      const s = sizes.get(p.id) ?? { width: 300, height: 96 }
      maxPromptHeight = Math.max(maxPromptHeight, s.height)
      totalWidth += s.width
    }
    totalWidth += Math.max(0, prompts.length - 1) * COLUMN_GAP

    const promptsTopY = headerY - VERT_GAP - maxPromptHeight
    const promptsStartX = headerCenterX - totalWidth / 2

    let cursorX = promptsStartX
    for (const p of prompts) {
      const s = sizes.get(p.id) ?? { width: 300, height: 96 }
      placedById.set(p.id, {
        ...p,
        position: { x: cursorX, y: promptsTopY } as XYPosition,
      })
      cursorX += s.width + COLUMN_GAP
    }

    laneLabelNodes.push({
      id: 'lane:prompt',
      type: 'lane-label',
      position: {
        x: promptsStartX,
        y: promptsTopY - LANE_LABEL_HEIGHT - LANE_LABEL_GAP,
      } as XYPosition,
      selectable: false,
      draggable: false,
      data: { label: CATEGORY_LABELS.prompt, count: prompts.length },
      width: totalWidth,
    } as AgentGraphNode)
  }

  // 2/3. Tools (upper) + DBs (lower) share the same X start (right of header).
  const rightStartX = headerX + headerSize.width + HORIZ_GAP

  // 2. Tool category frames. Each frame is an absolute-positioned parent and
  // its tools are placed in relative coordinates inside it. The header → frame
  // edges defined in build-agent-graph attach to the frame's left target
  // handle, so the header pencil-ins one edge per category instead of one
  // per tool.
  //
  // Frames pack into multiple rows under a width budget instead of stretching
  // into a single ribbon — with ~14 categories on the Engineer agent the old
  // single-row layout extended several thousand pixels horizontally, dwarfing
  // the rest of the canvas. Wrapping keeps the tools area roughly square.
  const tools = grouped.tool
  if (tools.length > 0 && toolFrames.length > 0) {
    const FRAME_PAD_X = 12
    const FRAME_PAD_TOP = 26
    const FRAME_PAD_BOTTOM = 12
    const GROUP_GAP = 28
    const FRAME_ROW_GAP = 32
    // Width budget for the tools block. Tuned so the Engineer agent (~14
    // categories) lays out as ~3 rows rather than a single 4000+px ribbon.
    const TOOLS_AREA_WIDTH_BUDGET = 1500

    // Order frames by their human label so layout is stable across renders.
    const orderedFrames = [...toolFrames].sort((a, b) => {
      const la = ((a.data ?? {}) as { label?: string }).label ?? ''
      const lb = ((b.data ?? {}) as { label?: string }).label ?? ''
      return la.localeCompare(lb)
    })

    interface FrameLayoutInfo {
      frame: AgentGraphNode
      frameTools: AgentGraphNode[]
      frameWidth: number
      frameHeight: number
      groupColCount: number
      rowsPerCol: number
      colWidth: number
      colHeights: number[]
    }

    // Pass 1: measure each frame's intrinsic size so we can pack them.
    const frameInfos: FrameLayoutInfo[] = []
    for (const frame of orderedFrames) {
      const frameTools = toolsByFrameId.get(frame.id) ?? []
      if (frameTools.length === 0) continue

      const groupColCount = Math.max(
        1,
        Math.ceil(frameTools.length / MAX_TOOLS_PER_COLUMN),
      )
      const rowsPerCol = Math.ceil(frameTools.length / groupColCount)

      let colWidth = 0
      for (const t of frameTools) {
        const s = sizes.get(t.id) ?? { width: 240, height: 36 }
        colWidth = Math.max(colWidth, s.width)
      }

      const groupWidth =
        groupColCount * colWidth + Math.max(0, groupColCount - 1) * COLUMN_GAP

      let tallestColHeight = 0
      const colHeights: number[] = []
      for (let c = 0; c < groupColCount; c++) {
        const subStart = c * rowsPerCol
        const subEnd = Math.min(subStart + rowsPerCol, frameTools.length)
        const colTools = frameTools.slice(subStart, subEnd)
        let subHeight = 0
        for (const t of colTools) {
          const s = sizes.get(t.id) ?? { width: colWidth, height: 36 }
          subHeight += s.height
        }
        subHeight += Math.max(0, colTools.length - 1) * ROW_GAP
        colHeights.push(subHeight)
        tallestColHeight = Math.max(tallestColHeight, subHeight)
      }

      frameInfos.push({
        frame,
        frameTools,
        frameWidth: groupWidth + FRAME_PAD_X * 2,
        frameHeight: tallestColHeight + FRAME_PAD_TOP + FRAME_PAD_BOTTOM,
        groupColCount,
        rowsPerCol,
        colWidth,
        colHeights,
      })
    }

    // Pass 2: greedily pack frames into rows under the width budget. Frames
    // wider than the budget claim a row of their own.
    const packedRows: FrameLayoutInfo[][] = [[]]
    let cursorRowWidth = 0
    for (const info of frameInfos) {
      const projected =
        cursorRowWidth +
        (packedRows[packedRows.length - 1].length > 0 ? GROUP_GAP : 0) +
        info.frameWidth
      if (
        projected > TOOLS_AREA_WIDTH_BUDGET &&
        packedRows[packedRows.length - 1].length > 0
      ) {
        packedRows.push([])
        cursorRowWidth = 0
      }
      const row = packedRows[packedRows.length - 1]
      row.push(info)
      cursorRowWidth +=
        info.frameWidth + (row.length > 1 ? GROUP_GAP : 0)
    }

    // Pass 3: place rows from bottom up so the lowest row sits at the same
    // baseline (colBottomY) the single-row layout used to occupy.
    const colBottomY = headerCenterY - HEADER_BAND_GAP
    let blockTopY = Number.POSITIVE_INFINITY
    let blockRightX = rightStartX
    let rowBottomY = colBottomY

    for (let r = packedRows.length - 1; r >= 0; r--) {
      const row = packedRows[r]
      if (row.length === 0) continue
      const rowHeight = row.reduce(
        (acc, info) => Math.max(acc, info.frameHeight),
        0,
      )
      let xCursor = rightStartX

      for (const info of row) {
        // Bottom-align each frame within the row so columns of different
        // heights end on a shared baseline.
        const frameX = xCursor
        const frameY = rowBottomY - info.frameHeight

        placedById.set(info.frame.id, {
          ...info.frame,
          position: { x: frameX, y: frameY } as XYPosition,
          width: info.frameWidth,
          height: info.frameHeight,
        } as AgentGraphNode)

        // Place tools relative to the frame. Bottom-aligned within the frame
        // so shorter columns hug the baseline rather than floating up.
        const interiorBottomYRel = info.frameHeight - FRAME_PAD_BOTTOM
        for (let c = 0; c < info.groupColCount; c++) {
          const subStart = c * info.rowsPerCol
          const subEnd = Math.min(
            subStart + info.rowsPerCol,
            info.frameTools.length,
          )
          const colTools = info.frameTools.slice(subStart, subEnd)
          const subHeight = info.colHeights[c]
          let yRel = interiorBottomYRel - subHeight
          const xRel = FRAME_PAD_X + c * (info.colWidth + COLUMN_GAP)

          for (const t of colTools) {
            const s = sizes.get(t.id) ?? { width: info.colWidth, height: 36 }
            placedById.set(t.id, {
              ...t,
              position: { x: xRel, y: yRel } as XYPosition,
            })
            yRel += s.height + ROW_GAP
          }
        }

        blockTopY = Math.min(blockTopY, frameY)
        blockRightX = Math.max(blockRightX, frameX + info.frameWidth)

        xCursor += info.frameWidth + GROUP_GAP
      }

      rowBottomY -= rowHeight + FRAME_ROW_GAP
    }

    laneLabelNodes.push({
      id: 'lane:tool',
      type: 'lane-label',
      position: {
        x: rightStartX,
        y: blockTopY - LANE_LABEL_HEIGHT - LANE_LABEL_GAP,
      } as XYPosition,
      selectable: false,
      draggable: false,
      data: { label: CATEGORY_LABELS.tool, count: tools.length },
      width: blockRightX - rightStartX,
    } as AgentGraphNode)
  }

  // 3. DBs — wrap into columns. Top of the block sits a small gap below the
  // header centre, so the block extends downward.
  const dbs = grouped['db-table']
  if (dbs.length > 0) {
    const colCount = Math.max(1, Math.ceil(dbs.length / MAX_DBS_PER_COLUMN))
    const rowsPerCol = Math.ceil(dbs.length / colCount)

    let colWidth = 0
    for (const d of dbs) {
      const s = sizes.get(d.id) ?? { width: 260, height: 104 }
      colWidth = Math.max(colWidth, s.width)
    }

    const blockTopY = headerCenterY + HEADER_BAND_GAP
    let blockRightX = rightStartX

    for (let c = 0; c < colCount; c++) {
      const subStart = c * rowsPerCol
      const subEnd = Math.min(subStart + rowsPerCol, dbs.length)
      const colDbs = dbs.slice(subStart, subEnd)

      let y = blockTopY
      const x = rightStartX + c * (colWidth + COLUMN_GAP)

      for (const d of colDbs) {
        const s = sizes.get(d.id) ?? { width: colWidth, height: 104 }
        placedById.set(d.id, {
          ...d,
          position: { x, y } as XYPosition,
        })
        y += s.height + ROW_GAP
      }

      blockRightX = Math.max(blockRightX, x + colWidth)
    }

    laneLabelNodes.push({
      id: 'lane:db-table',
      type: 'lane-label',
      position: {
        x: rightStartX,
        y: blockTopY - LANE_LABEL_HEIGHT - LANE_LABEL_GAP,
      } as XYPosition,
      selectable: false,
      draggable: false,
      data: { label: CATEGORY_LABELS['db-table'], count: dbs.length },
      width: blockRightX - rightStartX,
    } as AgentGraphNode)
  }

  // 4. Output(s) — row below header, centred on the header's x centre.
  const outputs = grouped['agent-output']
  let outputBottomY = headerY + headerSize.height + VERT_GAP
  if (outputs.length > 0) {
    let maxOutputHeight = 0
    let totalWidth = 0
    for (const o of outputs) {
      const s = sizes.get(o.id) ?? { width: 300, height: 110 }
      maxOutputHeight = Math.max(maxOutputHeight, s.height)
      totalWidth += s.width
    }
    totalWidth += Math.max(0, outputs.length - 1) * COLUMN_GAP

    const outputY = headerY + headerSize.height + VERT_GAP
    const outputStartX = headerCenterX - totalWidth / 2

    let cursorX = outputStartX
    for (const o of outputs) {
      const s = sizes.get(o.id) ?? { width: 300, height: 110 }
      placedById.set(o.id, {
        ...o,
        position: { x: cursorX, y: outputY } as XYPosition,
      })
      cursorX += s.width + COLUMN_GAP
    }

    laneLabelNodes.push({
      id: 'lane:agent-output',
      type: 'lane-label',
      position: {
        x: outputStartX,
        y: outputY - LANE_LABEL_HEIGHT - LANE_LABEL_GAP,
      } as XYPosition,
      selectable: false,
      draggable: false,
      data: { label: CATEGORY_LABELS['agent-output'], count: outputs.length },
      width: totalWidth,
    } as AgentGraphNode)
    outputBottomY = outputY + maxOutputHeight
  }

  // 5. Output sections — grid below the output node, centred on header centre.
  // Column count is capped so the grid never intrudes into the DB column on
  // the right (a 3-wide grid for 10 Plan sections is 632px wide and would
  // overlap the DB cards otherwise).
  const sections = grouped['output-section']
  if (sections.length > 0) {
    let cellWidth = 0
    let cellHeight = 0
    for (const s of sections) {
      const sz = sizes.get(s.id) ?? { width: 200, height: 64 }
      cellWidth = Math.max(cellWidth, sz.width)
      cellHeight = Math.max(cellHeight, sz.height)
    }
    const SECTION_RIGHT_SLACK = 40
    const dbColumnLeftEdge =
      dbs.length > 0 ? rightStartX - SECTION_RIGHT_SLACK : Number.POSITIVE_INFINITY
    // The grid is centred on headerCenterX, so its half-extent must fit in the
    // distance from headerCenterX to the DB column edge.
    const halfWidthBudget = Math.max(
      cellWidth,
      dbColumnLeftEdge - headerCenterX,
    )
    const maxGridWidth = halfWidthBudget * 2
    const maxColsByWidth = Math.max(
      1,
      Math.floor((maxGridWidth + SECTION_GRID_GAP) / (cellWidth + SECTION_GRID_GAP)),
    )
    const cols = Math.min(SECTION_GRID_COLS, sections.length, maxColsByWidth)
    const gridWidth = cols * cellWidth + (cols - 1) * SECTION_GRID_GAP
    const gridStartX = headerCenterX - gridWidth / 2
    const gridStartY = outputBottomY + VERT_GAP

    sections.forEach((sectionNode, idx) => {
      const sz = sizes.get(sectionNode.id) ?? { width: cellWidth, height: cellHeight }
      const col = idx % cols
      const row = Math.floor(idx / cols)
      const x = gridStartX + col * (cellWidth + SECTION_GRID_GAP)
      const y = gridStartY + row * (cellHeight + SECTION_GRID_GAP)
      placedById.set(sectionNode.id, {
        ...sectionNode,
        position: {
          x: x + (cellWidth - sz.width) / 2,
          y,
        } as XYPosition,
      })
    })

    laneLabelNodes.push({
      id: 'lane:output-section',
      type: 'lane-label',
      position: {
        x: gridStartX,
        y: gridStartY - LANE_LABEL_HEIGHT - LANE_LABEL_GAP,
      } as XYPosition,
      selectable: false,
      draggable: false,
      data: { label: CATEGORY_LABELS['output-section'], count: sections.length },
      width: gridWidth,
    } as AgentGraphNode)
  }

  // 6. Consumed artifacts — column on the left of the header, centred on the
  // header centre line.
  const consumed = grouped['consumed-artifact']
  if (consumed.length > 0) {
    let colWidth = 0
    let totalHeight = 0
    for (const a of consumed) {
      const s = sizes.get(a.id) ?? { width: 260, height: 96 }
      colWidth = Math.max(colWidth, s.width)
      totalHeight += s.height
    }
    totalHeight += Math.max(0, consumed.length - 1) * ROW_GAP

    const xRight = headerX - HORIZ_GAP
    const xLeft = xRight - colWidth
    const blockTopY = headerCenterY - totalHeight / 2

    let y = blockTopY
    for (const a of consumed) {
      const s = sizes.get(a.id) ?? { width: colWidth, height: 96 }
      placedById.set(a.id, {
        ...a,
        position: {
          x: xLeft + (colWidth - s.width) / 2,
          y,
        } as XYPosition,
      })
      y += s.height + ROW_GAP
    }

    laneLabelNodes.push({
      id: 'lane:consumed-artifact',
      type: 'lane-label',
      position: {
        x: xLeft,
        y: blockTopY - LANE_LABEL_HEIGHT - LANE_LABEL_GAP,
      } as XYPosition,
      selectable: false,
      draggable: false,
      data: { label: CATEGORY_LABELS['consumed-artifact'], count: consumed.length },
      width: colWidth,
    } as AgentGraphNode)
  }

  // Re-emit nodes in the original order so React keys stay stable, then append lanes.
  const ordered = nodes.map((node) => placedById.get(node.id) ?? node)
  return [...ordered, ...laneLabelNodes]
}

export function layoutAgentGraph<TNode extends Node, TEdge extends Edge>(
  nodes: TNode[],
  edges: TEdge[],
  sizes: Map<string, NodeSize> = new Map(),
  options: LayoutOptions = {},
): TNode[] {
  const merged = { ...DEFAULTS, ...options }
  const graph = new dagre.graphlib.Graph({ multigraph: false, compound: false })
  graph.setGraph({
    rankdir: merged.rankdir,
    ranksep: merged.ranksep,
    nodesep: merged.nodesep,
    edgesep: merged.edgesep,
    marginx: 12,
    marginy: 12,
  })
  graph.setDefaultEdgeLabel(() => ({}))

  for (const node of nodes) {
    const size = sizes.get(node.id) ?? merged.defaultSize
    graph.setNode(node.id, { width: size.width, height: size.height })
  }
  for (const edge of edges) {
    if (graph.hasNode(edge.source) && graph.hasNode(edge.target)) {
      graph.setEdge(edge.source, edge.target)
    }
  }

  dagre.layout(graph)

  return nodes.map((node) => {
    const placed = graph.node(node.id)
    if (!placed) return node
    const size = sizes.get(node.id) ?? merged.defaultSize
    const position: XYPosition = {
      x: placed.x - size.width / 2,
      y: placed.y - size.height / 2,
    }
    return { ...node, position } as TNode
  })
}
