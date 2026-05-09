import type { ListProjectFilesResponseDto, ProjectFileTreeNodeDto } from '@/src/lib/xero-model/project'
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
  nodesByPath: Record<string, ProjectFileTreeNodeDto>
  childPathsByPath: Record<string, string[]>
}

export interface ProjectFileTreeStoreStats {
  byteSize: number
  childListCount: number
  nodeCount: number
  unloadedFolderCount: number
}

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
    omittedEntryCount: response.view.omittedEntryCount,
    truncated: response.view.truncated,
  }
}

export function applyProjectFileListing(
  current: ProjectFileTreeStore,
  response: ListProjectFilesResponseDto,
): ProjectFileTreeStore {
  const listingRootPath = response.view.rootPath
  const next: ProjectFileTreeStore = {
    nodesByPath: { ...current.nodesByPath },
    childPathsByPath: { ...current.childPathsByPath },
  }

  pruneExistingChildren(next, listingRootPath)
  for (const node of Object.values(response.view.nodesByPath)) {
    next.nodesByPath[node.path] = node
    if (node.type === 'file') {
      delete next.childPathsByPath[node.path]
    }
  }
  for (const [path, childPaths] of Object.entries(response.view.childPathsByPath)) {
    next.childPathsByPath[path] = [...childPaths]
  }

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
