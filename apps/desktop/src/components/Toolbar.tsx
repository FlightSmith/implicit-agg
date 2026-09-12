import { useRef } from "react";
import { useWorkspace } from "../state/store";

export function Toolbar() {
  const core = useWorkspace((s) => s.core);
  const viewMode = useWorkspace((s) => s.viewMode);
  const setViewMode = useWorkspace((s) => s.setViewMode);
  const quality = useWorkspace((s) => s.quality);
  const setQuality = useWorkspace((s) => s.setQuality);
  const loadDemo = useWorkspace((s) => s.loadDemo);
  const loadDocument = useWorkspace((s) => s.loadDocument);
  const undo = useWorkspace((s) => s.undo);
  const redo = useWorkspace((s) => s.redo);
  const undoDepth = useWorkspace((s) => s.undoStack.length);
  const redoDepth = useWorkspace((s) => s.redoStack.length);
  const fileRef = useRef<HTMLInputElement>(null);

  const download = (bytes: Uint8Array, filename: string, type: string) => {
    const url = URL.createObjectURL(new Blob([bytes as unknown as BlobPart], { type }));
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = filename;
    anchor.click();
    URL.revokeObjectURL(url);
  };

  return (
    <div className="toolbar" data-testid="toolbar">
      <button onClick={() => void loadDemo()} data-testid="load-demo">
        demo wing
      </button>
      <input
        ref={fileRef}
        type="file"
        accept=".json"
        style={{ display: "none" }}
        onChange={async (event) => {
          const file = event.target.files?.[0];
          if (!file) return;
          const text = await file.text();
          await loadDocument(text);
          event.target.value = "";
        }}
      />
      <button onClick={() => fileRef.current?.click()}>open JSON…</button>
      <button
        disabled={!core}
        data-testid="save-json"
        onClick={() => {
          if (!core) return;
          download(
            new TextEncoder().encode(core.documentJson()),
            `${core.meta().id}.json`,
            "application/json",
          );
        }}
      >
        save JSON
      </button>
      <span className="toolbar-sep" />
      <button
        disabled={!core}
        onClick={() => download(core!.exportMesh("stl", quality, viewMode === "full"), "wing.stl", "model/stl")}
        data-testid="export-stl"
      >
        export STL
      </button>
      <button
        disabled={!core}
        onClick={() => download(core!.exportMesh("glb", quality, viewMode === "full"), "wing.glb", "model/gltf-binary")}
      >
        export GLB
      </button>
      <span className="toolbar-sep" />
      <button disabled={!core || undoDepth === 0} onClick={undo} data-testid="undo">
        undo
      </button>
      <button disabled={!core || redoDepth === 0} onClick={redo} data-testid="redo">
        redo
      </button>
      <span className="toolbar-sep" />
      <div className="segmented">
        {(["half", "full"] as const).map((mode) => (
          <button
            key={mode}
            className={viewMode === mode ? "selected" : ""}
            onClick={() => setViewMode(mode)}
            data-testid={`view-${mode}`}
          >
            {mode}
          </button>
        ))}
      </div>
      <div className="segmented">
        {(["interactive", "export"] as const).map((candidate) => (
          <button
            key={candidate}
            className={quality === candidate ? "selected" : ""}
            onClick={() => setQuality(candidate)}
          >
            {candidate}
          </button>
        ))}
      </div>
    </div>
  );
}
