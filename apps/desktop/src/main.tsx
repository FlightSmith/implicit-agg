import { createRoot } from "react-dom/client";
import { App } from "./App";
import { useWorkspace } from "./state/store";
import "./styles.css";

createRoot(document.getElementById("root")!).render(<App />);

// Debugging affordance: inspect live store state from the console/tests.
declare global {
  interface Window {
    __store: typeof useWorkspace;
  }
}
window.__store = useWorkspace;
