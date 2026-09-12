import { useEffect } from "react";
import { BottomPanel } from "./components/BottomPanel";
import { DesignTree } from "./components/DesignTree";
import { Inspector } from "./components/Inspector";
import { Toolbar } from "./components/Toolbar";
import { Viewport } from "./components/Viewport";
import { useWorkspace } from "./state/store";

export function App() {
  const ready = useWorkspace((s) => s.ready);
  const loadError = useWorkspace((s) => s.loadError);
  const loadDemo = useWorkspace((s) => s.loadDemo);
  const undo = useWorkspace((s) => s.undo);
  const redo = useWorkspace((s) => s.redo);

  useEffect(() => {
    void loadDemo();
  }, [loadDemo]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey)) return;
      const key = event.key.toLowerCase();
      if (key === "z" && !event.shiftKey) {
        event.preventDefault();
        undo();
      } else if ((key === "z" && event.shiftKey) || key === "y") {
        event.preventDefault();
        redo();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [undo, redo]);

  if (loadError.length > 0) {
    return (
      <div className="load-error">
        <h2>document rejected</h2>
        {loadError.map((diagnostic, index) => (
          <div key={index}>
            [{diagnostic.code}] {diagnostic.message}
          </div>
        ))}
      </div>
    );
  }

  return (
    <div className="app">
      <Toolbar />
      <div className="workspace">
        <DesignTree />
        <Viewport />
        <Inspector />
      </div>
      {ready && <BottomPanel />}
    </div>
  );
}
