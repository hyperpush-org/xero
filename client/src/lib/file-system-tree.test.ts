import { describe, expect, it } from 'vitest'
import {
  applyProjectFileListing,
  createEmptyProjectFileTreeStore,
  getProjectFileTreeStoreStats,
  isFolderLoaded,
  materializeProjectFileTree,
  type ProjectFileTreeStore,
} from './file-system-tree'
import type { ListProjectFilesResponseDto, ProjectFileTreeViewDto } from './xero-model/project'

function listing(path: string, children: ListProjectFilesResponseDto['root']['children']): ListProjectFilesResponseDto {
  const root = {
    name: path === '/' ? 'root' : path.split('/').pop() ?? 'folder',
    path,
    type: 'folder' as const,
    children,
    childrenLoaded: true,
  }
  return {
    projectId: 'project-1',
    path,
    root,
    view: viewFromRoot(root),
    truncated: false,
    omittedEntryCount: 0,
  }
}

function viewFromRoot(root: ListProjectFilesResponseDto['root']): ProjectFileTreeViewDto {
  const nodesByPath: ProjectFileTreeViewDto['nodesByPath'] = {}
  const childPathsByPath: ProjectFileTreeViewDto['childPathsByPath'] = {}

  const ingest = (node: ListProjectFilesResponseDto['root']) => {
    nodesByPath[node.path] = {
      id: node.path,
      name: node.name,
      path: node.path,
      type: node.type,
      childrenLoaded: node.type === 'file' ? true : node.childrenLoaded ?? false,
      truncated: node.truncated ?? false,
      omittedEntryCount: node.omittedEntryCount ?? 0,
    }
    if (node.type === 'folder') {
      childPathsByPath[node.path] = node.children?.map((child) => child.path) ?? []
      node.children?.forEach(ingest)
    }
  }
  ingest(root)

  const childLists = Object.values(childPathsByPath)
  return {
    rootPath: root.path,
    nodesByPath,
    childPathsByPath,
    loadedPaths: Object.values(nodesByPath)
      .filter((node) => node.type === 'folder' && node.childrenLoaded)
      .map((node) => node.path),
    stats: {
      byteSize: 1,
      childListCount: childLists.length,
      nodeCount: Object.keys(nodesByPath).length,
      unloadedFolderCount: Object.values(nodesByPath).filter(
        (node) => node.type === 'folder' && !node.childrenLoaded,
      ).length,
    },
    truncated: root.truncated ?? false,
    omittedEntryCount: root.omittedEntryCount ?? 0,
  }
}

describe('project file tree store', () => {
  it('hydrates folder listings from Rust flat views without inventing unloaded descendants', () => {
    let store: ProjectFileTreeStore = createEmptyProjectFileTreeStore()
    store = applyProjectFileListing(
      store,
      listing('/', [
        { name: 'src', path: '/src', type: 'folder', childrenLoaded: false },
        { name: 'README.md', path: '/README.md', type: 'file', childrenLoaded: true },
      ]),
    )

    let tree = materializeProjectFileTree(store)
    expect(tree.children?.map((node) => node.path)).toEqual(['/src', '/README.md'])
    expect(tree.children?.[0]?.children).toBeUndefined()
    expect(isFolderLoaded(store, '/src')).toBe(false)

    store = applyProjectFileListing(
      store,
      listing('/src', [{ name: 'main.ts', path: '/src/main.ts', type: 'file', childrenLoaded: true }]),
    )

    tree = materializeProjectFileTree(store)
    expect(tree.children?.[0]?.children?.map((node) => node.path)).toEqual(['/src/main.ts'])
    expect(isFolderLoaded(store, '/src')).toBe(true)
  })

  it('keeps hydrated descendants when an ancestor folder refreshes', () => {
    const rootListing = listing('/', [
      { name: 'src', path: '/src', type: 'folder', childrenLoaded: false },
      { name: 'README.md', path: '/README.md', type: 'file', childrenLoaded: true },
    ])
    let store: ProjectFileTreeStore = createEmptyProjectFileTreeStore()

    store = applyProjectFileListing(store, rootListing)
    store = applyProjectFileListing(
      store,
      listing('/src', [{ name: 'main.ts', path: '/src/main.ts', type: 'file', childrenLoaded: true }]),
    )
    store = applyProjectFileListing(store, rootListing)

    const src = materializeProjectFileTree(store).children?.find((node) => node.path === '/src')
    expect(src?.children?.map((node) => node.path)).toEqual(['/src/main.ts'])
    expect(isFolderLoaded(store, '/src')).toBe(true)
  })

  it('reuses the existing store object when a folder refresh has no changes', () => {
    const response = listing('/', [
      { name: 'src', path: '/src', type: 'folder', childrenLoaded: false },
      { name: 'README.md', path: '/README.md', type: 'file', childrenLoaded: true },
    ])
    const store = applyProjectFileListing(createEmptyProjectFileTreeStore(), response)

    expect(applyProjectFileListing(store, response)).toBe(store)
  })

  it('replaces stale children when a folder is reloaded', () => {
    let store = createEmptyProjectFileTreeStore()
    store = applyProjectFileListing(
      store,
      listing('/', [
        { name: 'old.ts', path: '/old.ts', type: 'file', childrenLoaded: true },
        { name: 'src', path: '/src', type: 'folder', childrenLoaded: false },
      ]),
    )
    store = applyProjectFileListing(
      store,
      listing('/', [{ name: 'src', path: '/src', type: 'folder', childrenLoaded: false }]),
    )

    expect(materializeProjectFileTree(store).children?.map((node) => node.path)).toEqual(['/src'])
  })

  it('reports approximate retained bytes for hydrated project tree stores', () => {
    const store = applyProjectFileListing(
      createEmptyProjectFileTreeStore(),
      listing('/', [
        { name: 'src', path: '/src', type: 'folder', childrenLoaded: false },
        { name: 'README.md', path: '/README.md', type: 'file', childrenLoaded: true },
      ]),
    )

    expect(getProjectFileTreeStoreStats(store)).toMatchObject({
      childListCount: 2,
      nodeCount: 3,
      unloadedFolderCount: 1,
    })
    expect(getProjectFileTreeStoreStats(store).byteSize).toBeGreaterThan(0)
  })

  it('applies the Rust flat view instead of the legacy recursive root DTO', () => {
    let store = createEmptyProjectFileTreeStore()
    const response = listing('/', [])
    response.view = viewFromRoot({
      name: 'root',
      path: '/',
      type: 'folder',
      childrenLoaded: true,
      children: [{ name: 'src', path: '/src', type: 'folder', childrenLoaded: false }],
    })

    store = applyProjectFileListing(store, response)

    expect(materializeProjectFileTree(store).children?.map((node) => node.path)).toEqual(['/src'])
  })
})
