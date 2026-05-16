import { render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import {
  MARKDOWN_CODE_BLOCK_HIGHLIGHT_BYTE_LIMIT,
  MARKDOWN_SEGMENT_CACHE_MAX_BYTES,
  Markdown,
  getMarkdownSegmentStats,
  resetMarkdownSegmentCacheForTests,
  resetMermaidSvgCacheForTests,
} from './conversation-markdown'

const mermaidRenderMock = vi.fn(async (_id: string, _text: string) => ({
  svg: '<svg data-testid="mermaid-svg"><g></g></svg>',
}))
const mermaidInitializeMock = vi.fn()

vi.mock('mermaid', () => ({
  default: {
    initialize: (...args: unknown[]) => mermaidInitializeMock(...args),
    render: (id: string, text: string) => mermaidRenderMock(id, text),
  },
}))

describe('conversation markdown performance behavior', () => {
  it('memoizes fenced segment parsing by message id and text revision', () => {
    resetMarkdownSegmentCacheForTests()
    const text = ['Here is a block:', '```', 'plain code', '```', 'done'].join('\n')
    const { rerender } = render(<Markdown messageId="turn-1" text={text} />)

    rerender(<Markdown messageId="turn-1" text={text} />)
    rerender(<Markdown messageId="turn-1" text={text} />)

    expect(getMarkdownSegmentStats().parses).toBe(1)

    rerender(<Markdown messageId="turn-1" text={`${text}\nstreamed tail`} />)
    expect(getMarkdownSegmentStats().parses).toBe(2)
  })

  it('evicts fenced segment cache entries by retained byte budget', () => {
    resetMarkdownSegmentCacheForTests()
    const largeText = [
      'Here is a block:',
      '```txt',
      'x'.repeat(Math.ceil(MARKDOWN_SEGMENT_CACHE_MAX_BYTES / 5)),
      '```',
    ].join('\n')

    render(<Markdown messageId="turn-large-a" text={largeText} />)
    render(<Markdown messageId="turn-large-b" text={largeText.replace('x', 'y')} />)

    const stats = getMarkdownSegmentStats()
    expect(stats.byteSize).toBeLessThanOrEqual(MARKDOWN_SEGMENT_CACHE_MAX_BYTES)
    expect(stats.evictions).toBeGreaterThan(0)
  })

  it('renders very large code blocks as readable plain text', () => {
    const oversizedCode = 'x'.repeat(MARKDOWN_CODE_BLOCK_HIGHLIGHT_BYTE_LIMIT / 2 + 1)
    render(<Markdown messageId="turn-large" text={['```ts', oversizedCode, '```'].join('\n')} />)

    expect(screen.getByText('Plain')).toBeInTheDocument()
    expect(screen.getByText(oversizedCode)).toBeInTheDocument()
  })

  it('keeps streaming fenced code plain without showing fallback churn', () => {
    render(
      <Markdown
        messageId="turn-streaming"
        streaming
        text={['```ts', 'const message = "still streaming"', '```'].join('\n')}
      />,
    )

    expect(screen.getByText('const message = "still streaming"')).toBeInTheDocument()
    expect(screen.queryByText('Plain')).not.toBeInTheDocument()
  })

  it('renders dense markdown at the condensed agent-pane scale', () => {
    const text = [
      '# Dense heading',
      '',
      'Dense paragraph.',
      '',
      '```ts',
      'const compact = true',
      '```',
      '',
      '| Area | Size |',
      '| --- | --- |',
      '| answer | dense |',
    ].join('\n')

    const { container } = render(<Markdown messageId="dense-scale" text={text} scale="dense" />)

    expect(container.firstElementChild).toHaveClass('text-[12px]')
    expect(screen.getByText('Dense heading')).toHaveClass('text-[13.5px]')
    expect(container.querySelector('pre')).toHaveClass('text-[11.5px]')
    expect(container.querySelector('table')).toHaveClass('text-[11.5px]')
    expect(container.querySelector('thead th')).toHaveClass('text-[11.5px]')
  })
})

describe('conversation markdown table rendering', () => {
  it('renders a GFM table with headers and body cells', () => {
    const text = [
      'Here is a comparison:',
      '',
      '| Agent | Scope |',
      '| --- | --- |',
      '| Ask | observe |',
      '| Engineer | edit |',
      '',
      'after the table.',
    ].join('\n')

    const { container } = render(<Markdown messageId="turn-table" text={text} />)

    const table = container.querySelector('table')
    expect(table).not.toBeNull()
    const headerCells = container.querySelectorAll('thead th')
    expect(headerCells).toHaveLength(2)
    expect(headerCells[0].textContent).toBe('Agent')
    expect(headerCells[1].textContent).toBe('Scope')
    const bodyRows = container.querySelectorAll('tbody tr')
    expect(bodyRows).toHaveLength(2)
    expect(bodyRows[0].querySelectorAll('td')[1].textContent).toBe('observe')
    expect(bodyRows[1].querySelectorAll('td')[0].textContent).toBe('Engineer')
    expect(screen.getByText('after the table.')).toBeInTheDocument()
  })

  it('applies alignment classes from the separator row', () => {
    const text = [
      '| Left | Center | Right |',
      '| :--- | :---: | ---: |',
      '| a | b | c |',
    ].join('\n')

    const { container } = render(<Markdown messageId="turn-align" text={text} />)

    const headerCells = container.querySelectorAll('thead th')
    expect(headerCells[0].className).toContain('text-left')
    expect(headerCells[1].className).toContain('text-center')
    expect(headerCells[2].className).toContain('text-right')
  })

  it('preserves inline formatting inside table cells', () => {
    const text = [
      '| Field | Value |',
      '| --- | --- |',
      '| **bold** | `code` |',
      '| plain | [link](https://example.com) |',
    ].join('\n')

    const { container } = render(<Markdown messageId="turn-inline-cells" text={text} />)

    expect(container.querySelector('tbody strong')?.textContent).toBe('bold')
    expect(container.querySelector('tbody code')?.textContent).toBe('code')
    const link = container.querySelector('tbody a') as HTMLAnchorElement | null
    expect(link).not.toBeNull()
    expect(link?.getAttribute('href')).toBe('https://example.com')
  })

  it('does not render a table while only the header row has streamed', () => {
    const text = ['| Col A | Col B |', '| Col C | Col D |'].join('\n')

    const { container } = render(<Markdown messageId="turn-mid-stream" text={text} streaming />)

    expect(container.querySelector('table')).toBeNull()
    expect(container.textContent).toContain('| Col A | Col B |')
  })

  it('pads short body rows so each row has exactly the header column count', () => {
    const text = [
      '| A | B | C |',
      '| --- | --- | --- |',
      '| only-one |',
      '| 1 | 2 | 3 | 4 |',
    ].join('\n')

    const { container } = render(<Markdown messageId="turn-mismatch" text={text} />)

    const bodyRows = container.querySelectorAll('tbody tr')
    expect(bodyRows[0].querySelectorAll('td')).toHaveLength(3)
    expect(bodyRows[1].querySelectorAll('td')).toHaveLength(3)
  })
})

describe('conversation markdown mermaid rendering', () => {
  it('renders ```mermaid blocks as plain text while streaming', () => {
    resetMermaidSvgCacheForTests()
    mermaidRenderMock.mockClear()

    const text = ['```mermaid', 'flowchart TD', 'A --> B', '```'].join('\n')
    render(<Markdown messageId="turn-mermaid-streaming" text={text} streaming />)

    expect(screen.getByText('Streaming')).toBeInTheDocument()
    expect(screen.getByText(/flowchart TD/)).toBeInTheDocument()
    expect(mermaidRenderMock).not.toHaveBeenCalled()
  })

  it('renders the SVG returned by mermaid once streaming completes', async () => {
    resetMermaidSvgCacheForTests()
    mermaidRenderMock.mockClear()
    mermaidRenderMock.mockResolvedValueOnce({
      svg: '<svg data-testid="mermaid-flow"><g></g></svg>',
    })

    const text = ['```mermaid', 'flowchart TD', 'A --> B', '```'].join('\n')
    render(<Markdown messageId="turn-mermaid-final" text={text} />)

    await waitFor(() => {
      expect(screen.getByTestId('mermaid-flow')).toBeInTheDocument()
    })
    expect(mermaidRenderMock).toHaveBeenCalledTimes(1)
    const [, source] = mermaidRenderMock.mock.calls[0]
    expect(source).toContain('flowchart TD')
  })

  it('falls back to a plain code block with an error badge when mermaid throws', async () => {
    resetMermaidSvgCacheForTests()
    mermaidRenderMock.mockClear()
    mermaidRenderMock.mockRejectedValueOnce(new Error('Parse error on line 1'))

    const text = ['```mermaid', 'flowchart TD', 'A -->', '```'].join('\n')
    render(<Markdown messageId="turn-mermaid-error" text={text} />)

    await waitFor(() => {
      expect(screen.getByText('Mermaid syntax error')).toBeInTheDocument()
    })
    expect(screen.getByText('Parse error on line 1')).toBeInTheDocument()
    expect(screen.getByText(/A -->/)).toBeInTheDocument()
  })
})
