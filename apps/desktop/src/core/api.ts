//! The core API seam: the UI only ever talks to this interface. The current
//! backend is the engine compiled to WebAssembly; a Tauri IPC backend
//! implements the same commands without touching any UI code.

import type { DiagnosticDto } from "./types";
import { WasmCore } from "./wasm";

export interface DocumentMeta {
  id: string;
  name: string;
  lengthUnit: string;
  angleUnit: string;
}

export interface UpdateResultDto {
  committed: boolean;
  revision: number;
  currentRevision: number;
  diagnostics: DiagnosticDto[];
  affected: string[];
}

export type MeshQuality = "interactive" | "settled" | "export";

export interface FaceDto {
  kind: "panel" | "tipCap" | "rootCap";
  lowerStation: number | null;
  upperStation: number | null;
  mirrored: boolean;
}

export interface MeshDto {
  revision: number;
  wingId: string;
  stationIds: string[];
  vertices: Float32Array;
  indices: Uint32Array;
  faces: FaceDto[];
}

export interface SourceTraceDto {
  wingId: string;
  stations: string[];
  mirrored: boolean;
  description: string;
}

export interface MeshFailure {
  kind: "cancelled" | "stale" | "failed";
  diagnostics: DiagnosticDto[];
}

export type OpenFailure = { diagnostics: DiagnosticDto[] };

export interface ValueKinds {
  x: string;
  y: string;
  z: string;
  chord: string;
  twist: string;
}

export interface ValueBindings {
  x: string | null;
  y: string | null;
  z: string | null;
  chord: string | null;
  twist: string | null;
}

export interface StationRow {
  id: string;
  airfoil: string;
  x: number;
  y: number;
  z: number;
  chord: number;
  twist: number;
  valueKinds: ValueKinds;
  bindings: ValueBindings;
  tangency: StationTangency | null;
}

export interface WingStations {
  wingId: string;
  /** Interface names published by this wing (autocomplete candidates). */
  interfaces: string[];
  stations: StationRow[];
}

/** One tangency side of a station. */
export interface StationTangencySide {
  auto: boolean;
  direction: [number, number, number] | null;
  strength: number;
}

export interface StationTangency {
  left: StationTangencySide | null;
  right: StationTangencySide | null;
}

export interface PanelReport {
  leadingEdgeSweep: number;
  quarterChordSweep: number;
  trailingEdgeSweep: number;
  dihedral: number;
}

export interface BasisPair {
  half: number;
  full: number;
}

export interface PlanformReport {
  referenceArea: BasisPair;
  span: BasisPair;
  mac: number;
  macLe: [number, number, number];
  aspectRatio: number;
  taperRatio: number | null;
  panels: PanelReport[];
}

export interface VolumeReport {
  volume: BasisPair;
  wettedArea: BasisPair;
  centerOfVolume: [number, number, number];
}

export interface WingReport {
  wingId: string;
  planform: PlanformReport;
  volume: VolumeReport | null;
  meshStatistics: { vertices: number; triangles: number };
}

export interface AircraftReport {
  revision: number;
  wings: WingReport[];
}

/** The command surface shared by the WASM core and a future Tauri backend. */
export interface CoreApi {
  revision(): number;
  documentJson(): string;
  meta(): DocumentMeta;
  diagnostics(): DiagnosticDto[];
  parameters(): Record<string, number>;
  setParameter(id: string, value: number, transaction: number): UpdateResultDto;
  addParameter(id: string, value: number, transaction: number): UpdateResultDto;
  setStationTangency(
    componentIndex: number,
    stationIndex: number,
    tangency: StationTangency | null,
    transaction: number,
  ): UpdateResultDto;
  /** Analytic STEP document of the wing (text, metres). */
  exportStep(componentIndex: number, fullModel: boolean): string;
  setStationField(
    componentIndex: number,
    stationIndex: number,
    field: string,
    value: unknown,
    transaction: number,
  ): UpdateResultDto;
  stations(): WingStations[];
  mesh(quality: MeshQuality, fullModel: boolean): MeshDto;
  trace(triangleIndex: number, fullModel: boolean): SourceTraceDto;
  report(): AircraftReport;
  exportMesh(
    format: "stl" | "obj" | "glb",
    quality: MeshQuality,
    fullModel: boolean,
  ): Uint8Array;
  snapshot(): CoreApi;
  restore(other: CoreApi): void;
}

/** Open a document; throws OpenFailure-shaped errors when it is rejected. */
export async function openCore(json: string): Promise<CoreApi> {
  return WasmCore.open(json);
}

export function isMeshFailure(error: unknown): MeshFailure | null {
  if (
    typeof error === "object" &&
    error !== null &&
    "kind" in error &&
    "diagnostics" in error
  ) {
    return error as MeshFailure;
  }
  return null;
}

export function isOpenFailure(error: unknown): OpenFailure | null {
  if (
    typeof error === "object" &&
    error !== null &&
    "diagnostics" in error
  ) {
    return error as OpenFailure;
  }
  return null;
}
