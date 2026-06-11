export type NativeTitleSuppressionHandle = {
  dispose: () => void
}

type NativeTitleSuppressionRoot = Document | Element

const noopHandle: NativeTitleSuppressionHandle = {
  dispose: () => {},
}

const interactiveRoles = new Set([
  'button',
  'checkbox',
  'link',
  'menuitem',
  'option',
  'radio',
  'switch',
  'tab',
  'treeitem',
])

export function installNativeTitleSuppression(
  root: NativeTitleSuppressionRoot | null = typeof document === 'undefined' ? null : document,
): NativeTitleSuppressionHandle {
  if (!root) return noopHandle

  const rootElement = root.nodeType === Node.DOCUMENT_NODE ? (root as Document).documentElement : (root as Element)
  const ownerDocument = rootElement?.ownerDocument

  if (!rootElement || !ownerDocument) return noopHandle

  const promotedAriaLabels = new WeakMap<Element, string>()
  const suppressElement = (element: Element) => {
    const nativeTitle = element.getAttribute('title')

    if (nativeTitle === null) return

    promoteNativeTitleLabel(element, nativeTitle, promotedAriaLabels)
    element.removeAttribute('title')
  }
  const suppressTree = (element: Element) => {
    suppressElement(element)
    element.querySelectorAll('[title]').forEach(suppressElement)
  }

  suppressTree(rootElement)

  const Observer = ownerDocument.defaultView?.MutationObserver ?? globalThis.MutationObserver
  if (!Observer) return noopHandle

  const observer = new Observer((mutations) => {
    for (const mutation of mutations) {
      if (mutation.type === 'attributes') {
        suppressElement(mutation.target as Element)
        continue
      }

      mutation.addedNodes.forEach((node) => {
        if (node.nodeType === Node.ELEMENT_NODE) {
          suppressTree(node as Element)
        }
      })
    }
  })

  observer.observe(rootElement, {
    attributeFilter: ['title'],
    attributes: true,
    childList: true,
    subtree: true,
  })

  let disposed = false

  return {
    dispose: () => {
      if (disposed) return
      disposed = true
      observer.disconnect()
    },
  }
}

function promoteNativeTitleLabel(
  element: Element,
  nativeTitle: string,
  promotedAriaLabels: WeakMap<Element, string>,
) {
  const promotedAriaLabel = promotedAriaLabels.get(element)
  const currentAriaLabel = element.getAttribute('aria-label')

  if (promotedAriaLabel !== undefined) {
    if (currentAriaLabel === promotedAriaLabel) {
      element.setAttribute('aria-label', nativeTitle)
      promotedAriaLabels.set(element, nativeTitle)
    } else {
      promotedAriaLabels.delete(element)
    }
    return
  }

  if (hasExplicitAccessibleName(element) || !shouldPromoteTitleToLabel(element)) {
    return
  }

  element.setAttribute('aria-label', nativeTitle)
  promotedAriaLabels.set(element, nativeTitle)
}

function hasExplicitAccessibleName(element: Element) {
  return element.hasAttribute('aria-label') || element.hasAttribute('aria-labelledby')
}

function shouldPromoteTitleToLabel(element: Element) {
  const tagName = element.tagName.toLowerCase()

  if (tagName === 'iframe' || tagName === 'canvas') return true
  if (tagName === 'img') return !element.hasAttribute('alt')
  if (tagName === 'svg') return true
  if (!isInteractiveElement(element)) return false

  return !hasTextContent(element)
}

function isInteractiveElement(element: Element) {
  const tagName = element.tagName.toLowerCase()
  const role = element.getAttribute('role')

  if (role && interactiveRoles.has(role)) return true

  return (
    tagName === 'a' ||
    tagName === 'button' ||
    tagName === 'input' ||
    tagName === 'select' ||
    tagName === 'summary' ||
    tagName === 'textarea'
  )
}

function hasTextContent(element: Element) {
  return (element.textContent ?? '').trim().length > 0
}
