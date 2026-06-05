import { Toaster } from '@xero/ui/components/ui/toaster'
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import { ShortcutsProvider } from './features/shortcuts/shortcuts-provider'
import { ThemeProvider } from './features/theme/theme-provider'
import { installNativeTitleSuppression } from './lib/native-title-suppression'
import './styles.css'

const container = document.getElementById('root')

if (!container) {
  throw new Error('Xero desktop shell root container was not found.')
}

installNativeTitleSuppression()

createRoot(container).render(
  <StrictMode>
    <ThemeProvider>
      <ShortcutsProvider>
        <App />
        <Toaster />
      </ShortcutsProvider>
    </ThemeProvider>
  </StrictMode>,
)
