import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import { ThemeProvider } from './features/theme/theme-provider'
import '../styles/globals.css'

const container = document.getElementById('root')

if (!container) {
  throw new Error('Xero desktop shell root container was not found.')
}

createRoot(container).render(
  <StrictMode>
    <ThemeProvider>
      <App />
    </ThemeProvider>
  </StrictMode>,
)
