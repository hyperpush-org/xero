import type { ListProjectFilesResponseDto, ProjectFileNodeDto } from '@/src/lib/xero-model/project'

export interface FileSystemNode {
  id: string
  name: string
  type: 'file' | 'folder'
  path: string
  children?: FileSystemNode[]
}

export function createEmptyFileSystem(): FileSystemNode {
  return {
    id: '/',
    name: 'root',
    type: 'folder',
    path: '/',
    children: [],
  }
}

export function mapProjectFileTree(response: ListProjectFilesResponseDto): FileSystemNode {
  return mapProjectFileNode(response.root)
}

export function mapProjectFileNode(node: ProjectFileNodeDto): FileSystemNode {
  return {
    id: node.path,
    name: node.name,
    type: node.type,
    path: node.path,
    children: node.children?.map(mapProjectFileNode),
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
