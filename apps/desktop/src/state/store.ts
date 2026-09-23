//! Application state: the engine handle lives outside React; the store
//! tracks derived snapshots (stations, parameters, diagnostics, report,
//! mesh) and the undo/redo stacks of committed snapshots.

import { create } from "zustand";
import type {
  AircraftReport,
  CoreApi,
  DocumentMeta,
  MeshDto,
  MeshQuality,
  WingStations,
} from "../core/api";
import { isMeshFailure, openCore } from "../core/api";
import type { WingCatalog } from "../core/controlPolicy";
import { WasmCore } from "../core/wasm";
import type { DiagnosticDto } from "../core/types";

export type BottomTab = "report" | "trace" | "diagnostics";

export interface TraceFace {
  kind: "panel" | "tipCap" | "rootCap";
  lowerStation: number | null;
  upperStation: number | null;
  mirrored: boolean;
}

export interface TraceState {
  triangleIndex: number;
  face: TraceFace;
  wingId: string;
  stations: string[];
  mirrored: boolean;
  description: string;
}

interface WorkspaceState {
  ready: boolean;
  core: CoreApi | null;
  wingCatalog: WingCatalog | null;
  loadError: DiagnosticDto[];
  meta: DocumentMeta | null;
  revision: number;
  diagnostics: DiagnosticDto[];
  parameters: Record<string, number>;
  stations: WingStations[];
  report: AircraftReport | null;
  mesh: MeshDto | null;
  meshPending: boolean;
  meshTier: "draft" | "settled";
  viewMode: "half" | "full";
  quality: MeshQuality;
  selectedStation: number;
  selectedWing: number;
  trace: TraceState | null;
  bottomTab: BottomTab;
  lastEffect: string | null;
  undoStack: CoreApi[];
  redoStack: CoreApi[];
  transactionCounter: number;

  loadDemo(): Promise<void>;
  loadDocument(json: string): Promise<void>;
  refresh(): void;
  requestMesh(): void;
  scheduleSettle(): void;
  remapTrace(mesh: MeshDto): void;
  setViewMode(mode: "half" | "full"): void;
  setQuality(quality: MeshQuality): void;
  selectStation(wing: number, station: number): void;
  selectTrace(triangleIndex: number): void;
  setBottomTab(tab: BottomTab): void;
  beginEdit(): void;
  endEdit(): void;
  commit(fn: (core: CoreApi) => { committed: boolean }): void;
  undo(): void;
  redo(): void;
}

let meshTimer: ReturnType<typeof setTimeout> | null = null;
let settleTimer: ReturnType<typeof setTimeout> | null = null;
let editInProgress = false;

function clearSettle() {
  if (settleTimer) clearTimeout(settleTimer);
  settleTimer = null;
}

export const useWorkspace = create<WorkspaceState>((set, get) => ({
  ready: false,
  core: null,
  wingCatalog: null,
  loadError: [],
  meta: null,
  revision: 0,
  diagnostics: [],
  parameters: {},
  stations: [],
  report: null,
  mesh: null,
  meshPending: false,
  meshTier: "draft",
  viewMode: "full",
  quality: "interactive",
  selectedStation: 0,
  selectedWing: 0,
  trace: null,
  bottomTab: "report",
  lastEffect: null,
  undoStack: [],
  redoStack: [],
  transactionCounter: 0,

  async loadDemo() {
    await get().loadDocument(await WasmCore.demoJson());
  },

  async loadDocument(json) {
    try {
      const core = await openCore(json);
      const wingCatalog = (await WasmCore.wingCatalog()) as WingCatalog;
      set({
        core,
        wingCatalog,
        ready: true,
        loadError: [],
        undoStack: [],
        redoStack: [],
      });
      get().refresh();
      get().requestMesh();
    } catch (error) {
      const diagnostics = (error as { diagnostics?: DiagnosticDto[] }).diagnostics;
      const loadError: DiagnosticDto[] =
        diagnostics && diagnostics.length > 0
          ? diagnostics
          : [
              {
                code: "open-failed",
                severity: "error",
                message: error instanceof Error ? error.message : String(error),
              },
            ];
      set({ ready: false, loadError });
    }
  },

  refresh() {
    const core = get().core;
    if (!core) return;
    set({
      meta: core.meta(),
      revision: core.revision(),
      diagnostics: core.diagnostics(),
      parameters: core.parameters(),
      stations: core.stations(),
      report: core.report(),
    });
  },

  requestMesh() {
    const core = get().core;
    if (!core) return;
    if (meshTimer) clearTimeout(meshTimer);
    clearSettle();
    set({ meshPending: true });
    // Let the pending state paint before the (synchronous) recompute.
    meshTimer = setTimeout(() => {
      try {
        const quality = get().quality;
        const mesh = core.mesh(quality, get().viewMode === "full");
        set({
          mesh,
          meshPending: false,
          meshTier: quality === "interactive" ? "draft" : "settled",
        });
        get().remapTrace(mesh);
        // docs/06: once the view rests, refine the draft into the settled,
        // export-quality tessellation.
        if (quality === "interactive") {
          get().scheduleSettle();
        }
      } catch (error) {
        const failure = isMeshFailure(error);
        const diagnostics = failure?.diagnostics ?? [];
        if (diagnostics.length > 0) {
          set({ meshPending: false, diagnostics, bottomTab: "diagnostics" });
        } else {
          set({ meshPending: false });
        }
      }
    }, 80);
  },

  /** Refine the resting preview to export-quality tessellation. */
  scheduleSettle() {
    clearSettle();
    settleTimer = setTimeout(() => {
      const core = get().core;
      if (!core) return;
      set({ meshPending: true });
      setTimeout(() => {
        try {
          const mesh = core.mesh("settled", get().viewMode === "full");
          set({ mesh, meshPending: false, meshTier: "settled" });
          get().remapTrace(mesh);
        } catch {
          // The draft mesh remains on display.
          set({ meshPending: false });
        }
      }, 20);
    }, 700);
  },

  /** Keep the selection highlight attached to the same feature when the
   * tessellation changes under it. */
  remapTrace(mesh: MeshDto) {
    const trace = get().trace;
    if (!trace) return;
    const face = trace.face;
    const index = mesh.faces.findIndex(
      (candidate) =>
        candidate.kind === face.kind &&
        candidate.lowerStation === face.lowerStation &&
        candidate.upperStation === face.upperStation &&
        candidate.mirrored === face.mirrored,
    );
    set({ trace: { ...trace, triangleIndex: index >= 0 ? index : -1 } });
  },

  setViewMode(mode) {
    set({ viewMode: mode, trace: null });
    get().requestMesh();
  },

  setQuality(quality) {
    set({ quality });
    get().requestMesh();
  },

  selectStation(wing, station) {
    set({ selectedWing: wing, selectedStation: station });
  },

  selectTrace(triangleIndex) {
    const core = get().core;
    const mesh = get().mesh;
    if (!core || !mesh) return;
    const face = mesh.faces[triangleIndex];
    if (!face) return;
    try {
      const source = core.trace(triangleIndex, get().viewMode === "full");
      set({
        trace: {
          triangleIndex,
          face: {
            kind: face.kind,
            lowerStation: face.lowerStation,
            upperStation: face.upperStation,
            mirrored: face.mirrored,
          },
          wingId: source.wingId,
          stations: source.stations,
          mirrored: source.mirrored,
          description: source.description,
        },
        bottomTab: "trace",
      });
    } catch {
      // Out-of-range pick: ignore.
    }
  },

  setBottomTab(tab) {
    set({ bottomTab: tab });
  },

  beginEdit() {
    const core = get().core;
    if (!core || editInProgress) return;
    editInProgress = true;
    set((state) => ({
      undoStack: [...state.undoStack.slice(-49), core.snapshot()],
      redoStack: [],
    }));
  },

  commit(fn) {
    const core = get().core;
    if (!core) return;
    const result = fn(core);
    // Rejected patches surface their diagnostics immediately; the committed
    // snapshot is unchanged.
    set({ diagnostics: core.diagnostics() });
    if (result.committed) {
      get().refresh();
      get().requestMesh();
    } else {
      set({ bottomTab: "diagnostics" });
    }
  },

  /** Close the current edit gesture; the next beginEdit opens a new undo
   * entry. Gesture handlers (pointer up, blur, Enter) call this. */
  endEdit() {
    editInProgress = false;
  },

  undo() {
    const state = get();
    const previous = state.undoStack[state.undoStack.length - 1];
    if (!previous || !state.core) return;
    // Snapshot the live state for redo, then revert to the previous
    // snapshot. Exactly one restore per undo.
    const current = state.core.snapshot();
    state.core.restore(previous);
    set({
      undoStack: state.undoStack.slice(0, -1),
      redoStack: [...state.redoStack, current],
      trace: null,
    });
    get().endEdit();
    get().refresh();
    get().requestMesh();
  },

  redo() {
    const state = get();
    const next = state.redoStack[state.redoStack.length - 1];
    if (!next || !state.core) return;
    const current = state.core.snapshot();
    state.core.restore(next);
    set({
      redoStack: state.redoStack.slice(0, -1),
      undoStack: [...state.undoStack, current],
      trace: null,
    });
    get().refresh();
    get().requestMesh();
  },
}));
