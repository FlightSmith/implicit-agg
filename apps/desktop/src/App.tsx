import { useEffect, useRef, useState } from "react";
import { BottomPanel } from "./components/BottomPanel";
import { DesignTree } from "./components/DesignTree";
import { Inspector } from "./components/Inspector";
import { Toolbar } from "./components/Toolbar";
import { Viewport } from "./components/Viewport";
import { useWorkspace } from "./state/store";

const MIN_PANE = 160;
const MAX_PANE = 520;

/** Draggable splitter between the workspace panes. */
function Splitter(props: { side: "left" | "right"; onResize: (delta: number) => void }) {
  const dragging = useRef(false);
  const lastX = useRef(0);

  return (
    <div
      className="splitter"
      role="separator"
      aria-orientation="vertical"
      onPointerDown={(event) => {
        dragging.current = true;
        lastX.current = event.clientX;
        event.currentTarget.setPointerCapture(event.pointerId);
      }}
      onPointerMove={(event) => {
        if (!dragging.current) return;
        props.onResize(event.clientX - lastX.current);
        lastX.current = event.clientX;
      }}
      onPointerUp={(event) => {
        dragging.current = false;
        event.currentTarget.releasePointerCapture(event.pointerId);
      }}
      data-testid={`splitter-${props.side}`}
    />
  );
}

export function App() {
  const ready = useWorkspace((s) => s.ready);
  const loadError = useWorkspace((s) => s.loadError);
  const loadDemo = useWorkspace((s) => s.loadDemo);
  const undo = useWorkspace((s) => s.undo);
  const redo = useWorkspace((s) => s.redo);
  const [treeWidth, setTreeWidth] = useState(220);
  const [inspectorWidth, setInspectorWidth] = useState(330);

  const clampPane = (width: number) =>
    Math.min(Math.max(width, MIN_PANE), MAX_PANE);

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
      <div
        className="workspace"
        style={{ gridTemplateColumns: `${treeWidth}px 5px 1fr 5px ${inspectorWidth}px` }}
      >
        <DesignTree />
        <Splitter side="left" onResize={(delta) => setTreeWidth((w) => clampPane(w + delta))} />
        <Viewport />
        <Splitter
          side="right"
          onResize={(delta) => setInspectorWidth((w) => clampPane(w - delta))}
        />
        <Inspector />
      </div>
      {ready && <BottomPanel />}
    </div>
  );
}
