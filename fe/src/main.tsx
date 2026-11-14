import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import Version from './Version.tsx'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <Version />
  </StrictMode>,
)
