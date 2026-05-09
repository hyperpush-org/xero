import '@testing-library/jest-dom/vitest'
import { cleanup } from '@testing-library/react'
import { afterEach, vi } from 'vitest'

afterEach(() => {
  cleanup()
  window.localStorage.clear()
  window.sessionStorage.clear()
})

function createMemoryStorage(): Storage {
  const store = new Map<string, string>()

  return {
    get length() {
      return store.size
    },
    clear() {
      store.clear()
    },
    getItem(key) {
      return store.has(key) ? store.get(key)! : null
    },
    key(index) {
      return Array.from(store.keys())[index] ?? null
    },
    removeItem(key) {
      store.delete(key)
    },
    setItem(key, value) {
      store.set(key, String(value))
    },
  }
}

Object.defineProperty(window, 'localStorage', {
  configurable: true,
  value: createMemoryStorage(),
})

Object.defineProperty(window, 'sessionStorage', {
  configurable: true,
  value: createMemoryStorage(),
})

Object.defineProperty(window.HTMLElement.prototype, 'scrollIntoView', {
  configurable: true,
  value: vi.fn(),
})

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}

Object.defineProperty(globalThis, 'ResizeObserver', {
  configurable: true,
  writable: true,
  value: ResizeObserverStub,
})

Object.defineProperty(window, 'ResizeObserver', {
  configurable: true,
  writable: true,
  value: ResizeObserverStub,
})

Object.defineProperty(window, 'matchMedia', {
  configurable: true,
  writable: true,
  value: vi.fn().mockImplementation((query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: vi.fn(),
  })),
})

if (typeof window.DOMMatrixReadOnly === 'undefined') {
  class DOMMatrixReadOnlyStub {
    m22 = 1

    constructor(transform?: string) {
      if (typeof transform !== 'string') return
      const matrix = transform.match(/^matrix\(([^)]+)\)$/)
      if (matrix) {
        const values = matrix[1].split(',').map((value) => Number.parseFloat(value.trim()))
        if (Number.isFinite(values[3])) this.m22 = values[3]
        return
      }

      const matrix3d = transform.match(/^matrix3d\(([^)]+)\)$/)
      if (matrix3d) {
        const values = matrix3d[1].split(',').map((value) => Number.parseFloat(value.trim()))
        if (Number.isFinite(values[5])) this.m22 = values[5]
      }
    }
  }

  Object.defineProperty(window, 'DOMMatrixReadOnly', {
    configurable: true,
    writable: true,
    value: DOMMatrixReadOnlyStub,
  })
  Object.defineProperty(globalThis, 'DOMMatrixReadOnly', {
    configurable: true,
    writable: true,
    value: DOMMatrixReadOnlyStub,
  })
}
