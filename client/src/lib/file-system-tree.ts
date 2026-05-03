import type { ListProjectFilesResponseDto, ProjectFileNodeDto } from '@/src/lib/xero-model/project'
import { estimateUtf16Bytes } from '@/lib/byte-budget-cache'

export interface FileSystemNode {
  id: string
  name: string
  type: 'file' | 'folder'
  path: string
  children?: FileSystemNode[]
  childrenLoaded?: boolean
  truncated?: boolean
  omittedEntryCount?: number
}

export interface ProjectFileTreeBudgetInfo {
  omittedEntryCount: number
  truncated: boolean
}

export interface ProjectFileTreeStore {
  nodesByPath: Record<string, Omit<FileSystemNode, 'children'>>
  childPathsByPath: Record<string, string[]>
}

export interface ProjectFileTreeStoreStats {
  byteSize: number
  childListCount: number
  nodeCount: number
  unloadedFolderCount: number
}

export interface TrimProjectFileTreeStoreResult {
  prunedFolderCount: number
  stats: ProjectFileTreeStoreStats
  store: ProjectFileTreeStore
}

export const DEFAULT_PROJECT_FILE_TREE_STORE_MAX_BYTES = 4 * 1024 * 1024

export function createEmptyFileSystem(): FileSystemNode {
  return {
    id: '/',
    name: 'root',
    type: 'folder',
    path: '/',
    children: [],
    childrenLoaded: false,
  }
}

export function createEmptyProjectFileTreeStore(): ProjectFileTreeStore {
  return {
    nodesByPath: {
      '/': {
        id: '/',
        name: 'root',
        type: 'folder',
        path: '/',
        childrenLoaded: false,
        truncated: false,
        omittedEntryCount: 0,
      },
    },
    childPathsByPath: {
      '/': [],
    },
  }
}

export function mapProjectFileTree(response: ListProjectFilesResponseDto): FileSystemNode {
  return materializeProjectFileTree(applyProjectFileListing(createEmptyProjectFileTreeStore(), response))
}

export function getProjectFileTreeBudgetInfo(response: ListProjectFilesResponseDto): ProjectFileTreeBudgetInfo {
  return {
    omittedEntryCount: response.omittedEntryCount ?? 0,
    truncated: response.truncated ?? false,
  }
}

export function mapProjectFileNode(node: ProjectFileNodeDto): FileSystemNode {
  const children = node.children?.map(mapProjectFileNode)
  return {
    id: node.path,
    name: node.name,
    type: node.type,
    path: node.path,
    children,
    childrenLoaded: node.type === 'file' ? true : node.childrenLoaded ?? Boolean(children),
    truncated: node.truncated ?? false,
    omittedEntryCount: node.omittedEntryCount ?? 0,
  }
}

export function applyProjectFileListing(
  current: ProjectFileTreeStore,
  response: ListProjectFilesResponseDto,
): ProjectFileTreeStore {
  const listingRoot = mapProjectFileNode(response.root)
  const next: ProjectFileTreeStore = {
    nodesByPath: { ...current.nodesByPath },
    childPathsByPath: { ...current.childPathsByPath },
  }

  pruneExistingChildren(next, listingRoot.path)
  ingestNode(next, listingRoot)

  return next
}

export function materializeProjectFileTree(store: ProjectFileTreeStore): FileSystemNode {
  return materializeNode(store, '/') ?? createEmptyFileSystem()
}

export function isFolderLoaded(store: ProjectFileTreeStore, path: string): boolean {
  return Boolean(store.nodesByPath[path]?.childrenLoaded)
}

export function getProjectFileTreeStoreStats(store: ProjectFileTreeStore): ProjectFileTreeStoreStats {
  let byteSize = 0
  let unloadedFolderCount = 0
  const nodes = Object.values(store.nodesByPath)

  for (const node of nodes) {
    byteSize += 48
    byteSize += estimateUtf16Bytes(node.id)
    byteSize += estimateUtf16Bytes(node.name)
    byteSize += estimateUtf16Bytes(node.path)
    byteSize += estimateUtf16Bytes(node.type)
    if (node.type === 'folder' && !node.childrenLoaded) {
      unloadedFolderCount += 1
    }
  }

  const childLists = Object.values(store.childPathsByPath)
  for (const childPaths of childLists) {
    byteSize += 24
    for (const childPath of childPaths) {
      byteSize += estimateUtf16Bytes(childPath) + 8
    }
  }

  return {
    byteSize,
    childListCount: childLists.length,
    nodeCount: nodes.length,
    unloadedFolderCount,
  }
}

export function trimProjectFileTreeStoreToBudget(
  store: ProjectFileTreeStore,
  options: {
    maxBytes?: number
    protectedPaths?: Iterable<string | null | undefined>
  } = {},
): TrimProjectFileTreeStoreResult {
  const maxBytes = options.maxBytes ?? DEFAULT_PROJECT_FILE_TREE_STORE_MAX_BYTES
  let stats = getProjectFileTreeStoreStats(store)
  if (stats.byteSize <= maxBytes) {
    return { prunedFolderCount: 0, stats, store }
  }

  const protectedPaths = collectProtectedFileTreePaths(store, options.protectedPaths ?? [])
  const candidates = Object.keys(store.childPathsByPath)
    .filter((path) => path !== '/' && !protectedPaths.has(path) && (store.childPathsByPath[path]?.length ?? 0) > 0)
    .sort((left, right) => countDescendants(store, right) - countDescendants(store, left))

  let next: ProjectFileTreeStore | null = null
  let prunedFolderCount = 0
  for (const path of candidates) {
    if (stats.byteSize <= maxBytes) {
      break
    }

    const working: ProjectFileTreeStore = next ?? {
      nodesByPath: { ...store.nodesByPath },
      childPathsByPath: { ...store.childPathsByPath },
    }
    const node = working.nodesByPath[path]
    if (!node || node.type !== 'folder') {
      next = working
      continue
    }

    pruneExistingChildren(working, path)
    working.nodesByPath[path] = {
      ...node,
      childrenLoaded: false,
    }
    next = working
    prunedFolderCount += 1
    stats = getProjectFileTreeStoreStats(working)
  }

  return {
    prunedFolderCount,
    stats,
    store: next ?? store,
  }
}

function collectProtectedFileTreePaths(
  store: ProjectFileTreeStore,
  paths: Iterable<string | null | undefined>,
): Set<string> {
  const protectedPaths = new Set<string>(['/'])
  for (const rawPath of paths) {
    if (!rawPath) continue
    const nodePath = rawPath.trim() || '/'
    if (!store.nodesByPath[nodePath]) continue
    let current = nodePath
    while (current.length > 0) {
      protectedPaths.add(current)
      if (current === '/') break
      const separatorIndex = current.lastIndexOf('/')
      current = separatorIndex <= 0 ? '/' : current.slice(0, separatorIndex)
    }
  }
  return protectedPaths
}

function countDescendants(store: ProjectFileTreeStore, path: string): number {
  let count = 0
  for (const childPath of store.childPathsByPath[path] ?? []) {
    count += 1 + countDescendants(store, childPath)
  }
  return count
}

function pruneExistingChildren(store: ProjectFileTreeStore, path: string): void {
  for (const childPath of store.childPathsByPath[path] ?? []) {
    removeNode(store, childPath)
  }
  store.childPathsByPath[path] = []
}

function removeNode(store: ProjectFileTreeStore, path: string): void {
  for (const childPath of store.childPathsByPath[path] ?? []) {
    removeNode(store, childPath)
  }
  delete store.nodesByPath[path]
  delete store.childPathsByPath[path]
}

function ingestNode(store: ProjectFileTreeStore, node: FileSystemNode): void {
  const { children, ...nodeWithoutChildren } = node
  store.nodesByPath[node.path] = nodeWithoutChildren
  if (node.type === 'folder') {
    const childPaths = children?.map((child) => child.path) ?? []
    store.childPathsByPath[node.path] = childPaths
    for (const child of children ?? []) {
      ingestNode(store, child)
    }
  } else {
    delete store.childPathsByPath[node.path]
  }
}

function materializeNode(store: ProjectFileTreeStore, path: string): FileSystemNode | null {
  const node = store.nodesByPath[path]
  if (!node) return null
  if (node.type !== 'folder') {
    return { ...node }
  }

  const children = (store.childPathsByPath[path] ?? [])
    .map((childPath) => materializeNode(store, childPath))
    .filter((child): child is FileSystemNode => Boolean(child))

  return {
    ...node,
    children: node.childrenLoaded ? children : children.length > 0 ? children : undefined,
  }
}

export function findNode(root: FileSystemNode, path: string): FileSystemNode | null {
  if (root.path === path) return root
  if (!root.children) return null

  for (const child of root.children) {
    const found = findNode(child, path)
    if (found) return found
  }

  return null
}

export function listAllFolderPaths(root: FileSystemNode): string[] {
  const paths: string[] = []

  function walk(node: FileSystemNode) {
    if (node.type === 'folder') {
      paths.push(node.path)
    }

    node.children?.forEach(walk)
  }

  walk(root)
  return paths
}
