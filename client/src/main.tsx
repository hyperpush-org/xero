import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import '../styles/globals.css'

const container = document.getElementById('root')

if (!container) {
  throw new Error('Cadence desktop shell root container was not found.')
}

createRoot(container).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
