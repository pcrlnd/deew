import { StrictMode } from "react"
import { createRoot } from "react-dom/client"

import "./index.css"
import ToolChain from "./ToolChain.tsx"

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ToolChain />
  </StrictMode>
)
