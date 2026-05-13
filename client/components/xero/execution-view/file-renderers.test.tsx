import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  CsvPreview,
  HtmlPreview,
  ImagePreview,
  MarkdownPreview,
  MediaPreview,
  PdfPreview,
  SvgTextPreview,
  UnsupportedFilePanel,
  parseDelimitedText,
} from './file-renderers'

describe('file renderers', () => {
  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('renders the image canvas and reports decoded dimensions through the controlled callback', async () => {
    const onDimensionsChange = vi.fn()
    render(
      <ImagePreview
        filePath="/assets/logo.png"
        src="project-asset://image-token"
        onDimensionsChange={onDimensionsChange}
      />,
    )

    const preview = screen.getByTestId('image-preview')
    const image = preview.querySelector('img')
    expect(image).toHaveAttribute('src', 'project-asset://image-token')
    // ImagePreview no longer owns a toolbar — controls live in EditorTopBar.
    expect(screen.queryByRole('toolbar')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Zoom in' })).not.toBeInTheDocument()

    Object.defineProperty(image!, 'naturalWidth', { configurable: true, value: 32 })
    Object.defineProperty(image!, 'naturalHeight', { configurable: true, value: 24 })
    fireEvent.load(image!)

    await waitFor(() =>
      expect(onDimensionsChange).toHaveBeenCalledWith({ width: 32, height: 24 }),
    )
  })

  it('renders SVG source through an image blob and revokes blob URLs on changes and unmount', async () => {
    const createObjectURL = vi.fn()
      .mockReturnValueOnce('blob:xero-svg-1')
      .mockReturnValueOnce('blob:xero-svg-2')
    const revokeObjectURL = vi.fn()
    Object.defineProperty(URL, 'createObjectURL', {
      configurable: true,
      value: createObjectURL,
    })
    Object.defineProperty(URL, 'revokeObjectURL', {
      configurable: true,
      value: revokeObjectURL,
    })

    const unsafeSvg = '<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>'
    const { container, rerender, unmount } = render(
      <SvgTextPreview filePath="/logo.svg" text={unsafeSvg} mimeType="image/svg+xml" />,
    )

    await waitFor(() => expect(createObjectURL).toHaveBeenCalledTimes(1))
    expect(container.querySelector('script')).not.toBeInTheDocument()
    expect(screen.getByAltText('SVG preview of logo.svg')).toHaveAttribute('src', 'blob:xero-svg-1')

    rerender(
      <SvgTextPreview
        filePath="/logo.svg"
        text='<svg xmlns="http://www.w3.org/2000/svg"><rect /></svg>'
        mimeType="image/svg+xml"
      />,
    )

    await waitFor(() => expect(revokeObjectURL).toHaveBeenCalledWith('blob:xero-svg-1'))
    expect(screen.getByAltText('SVG preview of logo.svg')).toHaveAttribute('src', 'blob:xero-svg-2')

    unmount()

    expect(revokeObjectURL).toHaveBeenCalledWith('blob:xero-svg-2')
  })

  it('sanitizes Markdown HTML, unsafe links, and traversal image references', async () => {
    const resolveAssetPreviewUrl = vi.fn(async (path: string) => `project-asset://preview${path}`)

    render(
      <MarkdownPreview
        filePath="/docs/guide.md"
        text={[
          '# Guide',
          '',
          '[bad](javascript:alert(1))',
          '![safe](./logo.png)',
          '![escape](../../secret.png)',
          '<script>window.xeroUnsafe = true</script>',
          '<img src=x onerror=alert(1)>',
        ].join('\n')}
        onResolveAssetPreviewUrl={resolveAssetPreviewUrl}
      />,
    )

    expect(await screen.findByRole('heading', { name: 'Guide' })).toBeVisible()
    expect(document.querySelector('script')).not.toBeInTheDocument()
    expect(screen.queryByRole('link', { name: 'bad' })).not.toBeInTheDocument()

    await waitFor(() => expect(resolveAssetPreviewUrl).toHaveBeenCalledWith('/docs/logo.png'))
    expect(resolveAssetPreviewUrl).not.toHaveBeenCalledWith('/secret.png')
    expect(await screen.findByAltText('safe')).toHaveAttribute(
      'src',
      'project-asset://preview/docs/logo.png',
    )
    expect(screen.getByText(/Image unavailable:/)).toHaveTextContent('../../secret.png')
  })

  it('renders saved Markdown images from Rust-provided asset refs without resolving in React', async () => {
    const resolveAssetPreviewUrl = vi.fn(async (path: string) => `project-asset://preview${path}`)

    render(
      <MarkdownPreview
        filePath="/docs/guide.md"
        text="![safe](./logo.png)\n\n![missing](./missing.png)"
        preview={{
          kind: 'markdown',
          assetRefs: [
            {
              source: './logo.png',
              path: '/docs/logo.png',
              previewUrl: 'project-asset://rust-logo',
            },
            {
              source: './missing.png',
              path: '/docs/missing.png',
              previewUrl: null,
            },
          ],
        }}
        onResolveAssetPreviewUrl={resolveAssetPreviewUrl}
      />,
    )

    expect(await screen.findByAltText('safe')).toHaveAttribute('src', 'project-asset://rust-logo')
    expect(resolveAssetPreviewUrl).not.toHaveBeenCalled()
    expect(screen.getByText(/Image unavailable:/)).toHaveTextContent('./missing.png')
    expect(screen.getByRole('status')).toHaveTextContent('saved image reference')
  })

  it('keeps saved Markdown asset refs while resolving unsaved image edits predictably', async () => {
    const resolveAssetPreviewUrl = vi.fn(async (path: string) => `project-asset://preview${path}`)

    render(
      <MarkdownPreview
        filePath="/docs/guide.md"
        text="![saved](./logo.png)\n\n![new](./new-logo.png)"
        preview={{
          kind: 'markdown',
          assetRefs: [
            {
              source: './logo.png',
              path: '/docs/logo.png',
              previewUrl: 'project-asset://rust-logo',
            },
          ],
        }}
        onResolveAssetPreviewUrl={resolveAssetPreviewUrl}
      />,
    )

    expect(await screen.findByAltText('saved')).toHaveAttribute('src', 'project-asset://rust-logo')
    await waitFor(() => expect(resolveAssetPreviewUrl).toHaveBeenCalledWith('/docs/new-logo.png'))
    expect(resolveAssetPreviewUrl).toHaveBeenCalledTimes(1)
    expect(await screen.findByAltText('new')).toHaveAttribute(
      'src',
      'project-asset://preview/docs/new-logo.png',
    )
  })

  it('renders CSV tables and keeps parser limits deterministic for quoted delimited text', () => {
    const parsed = parseDelimitedText('"name","count"\n"Alpha, Inc.",1\n"Beta ""B""",2', ',', {
      rowLimit: 2,
      columnLimit: 1,
    })

    expect(parsed.rows).toEqual([['name'], ['Alpha, Inc.']])
    expect(parsed.totalRows).toBe(3)
    expect(parsed.truncatedRows).toBe(true)
    expect(parsed.truncatedColumns).toBe(true)

    render(
      <CsvPreview
        filePath="/data.tsv"
        mimeType="text/tab-separated-values; charset=utf-8"
        text={'name\tcount\nAlpha\t1'}
      />,
    )

    expect(screen.getByTestId('csv-preview')).toHaveTextContent('2 rows')
    expect(screen.getByRole('table', { name: 'Table preview of /data.tsv' })).toBeVisible()
  })

  it('renders CSV previews from Rust-provided rows without parsing the text fallback', () => {
    render(
      <CsvPreview
        filePath="/data.csv"
        mimeType="text/csv; charset=utf-8"
        text="fallback,should-not-render"
        preview={{
          kind: 'csv',
          headers: ['name', 'count'],
          rows: [['Rust row', '3']],
          totalRows: 2,
          totalColumns: 2,
          truncatedRows: false,
          truncatedColumns: false,
        }}
      />,
    )

    expect(screen.getByTestId('csv-preview')).toHaveTextContent('Rust row')
    expect(screen.queryByText('fallback')).not.toBeInTheDocument()
  })

  it('renders PDF, audio, video, HTML preview, and unsupported file states with safe actions', async () => {
    const onCopyPath = vi.fn()
    const onOpenExternal = vi.fn()
    const { rerender } = render(
      <PdfPreview
        filePath="/paper.pdf"
        src="project-asset://pdf-token"
        onCopyPath={onCopyPath}
        onOpenExternal={onOpenExternal}
      />,
    )

    const object = screen.getByTestId('pdf-preview').querySelector('object')
    expect(object).toHaveAttribute('data', 'project-asset://pdf-token')
    expect(object).toHaveAttribute('type', 'application/pdf')
    fireEvent.click(screen.getByRole('button', { name: 'Open externally' }))
    expect(onOpenExternal).toHaveBeenCalledWith('/paper.pdf')

    rerender(
      <MediaPreview
        filePath="/theme.mp3"
        src="project-asset://audio-token"
        rendererKind="audio"
      />,
    )
    expect(screen.getByTestId('audio-preview').querySelector('audio')).toHaveAttribute(
      'src',
      'project-asset://audio-token',
    )

    rerender(
      <MediaPreview
        filePath="/demo.mp4"
        src="project-asset://video-token"
        rendererKind="video"
      />,
    )
    expect(screen.getByTestId('video-preview').querySelector('video')).toHaveAttribute(
      'src',
      'project-asset://video-token',
    )

    const resolveAssetPreviewUrl = vi.fn(async (path: string) => {
      if (path === '/assets/logo.png') return 'project-asset://preview/assets/logo.png'
      return null
    })
    rerender(
      <HtmlPreview
        filePath="/index.html"
        text={[
          '<!doctype html>',
          '<button onclick="alert(1)">Run</button>',
          '<script>window.xeroUnsafe = true</script>',
          '<img alt="Logo" src="./assets/logo.png">',
          '<img alt="Missing" src="./missing.png">',
        ].join('\n')}
        onResolveAssetPreviewUrl={resolveAssetPreviewUrl}
      />,
    )
    const htmlPreview = screen.getByTestId('html-preview')
    const iframe = htmlPreview.querySelector('iframe')
    expect(iframe).toHaveAttribute('sandbox', '')
    await waitFor(() => expect(resolveAssetPreviewUrl).toHaveBeenCalledWith('/assets/logo.png'))
    expect(iframe?.getAttribute('srcdoc')).not.toContain('<script>')
    expect(iframe?.getAttribute('srcdoc')).not.toContain('onclick')
    expect(iframe?.getAttribute('srcdoc')).toContain('project-asset://preview/assets/logo.png')
    expect(screen.getByRole('status')).toHaveTextContent('Removed 1 blocked element')
    expect(screen.getByRole('status')).toHaveTextContent('Image unavailable: ./missing.png')

    rerender(
      <UnsupportedFilePanel
        filePath="/archive.bin"
        byteLength={1024}
        contentHash="0123456789abcdef0123456789abcdef"
        modifiedAt="2026-01-01T00:00:00Z"
        mimeType="application/octet-stream"
        reason="binary"
        rendererKind={null}
        onCopyPath={onCopyPath}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Copy path' }))
    expect(onCopyPath).toHaveBeenCalledWith('/archive.bin')
    expect(screen.getByTestId('unsupported-file-panel')).toHaveTextContent('0123456789abcdef...')
  })
})
