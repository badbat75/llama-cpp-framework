import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App.tsx'
import './index.css'
import { startTheme } from './theme.ts'

// The stored choice goes on `<html>` before the first render, so the first
// paint carries the theme the user chose (07-hearth-h4-theme.md, H4.4).
startTheme()

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
