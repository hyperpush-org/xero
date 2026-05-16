import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import { ShortcutsProvider } from './features/shortcuts/shortcuts-provider'
import { ThemeProvider } from './features/theme/theme-provider'
import '@xero/ui/styles.css'

const container = document.getElementById('root')

if (!container) {
  throw new Error('Xero desktop shell root container was not found.')
}

createRoot(container).render(
  <StrictMode>
    <ThemeProvider>
      <ShortcutsProvider>
        <App />
      </ShortcutsProvider>
    </ThemeProvider>
  </StrictMode>,
)
