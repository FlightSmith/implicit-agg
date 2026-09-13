//! WASM-backed implementation of the CoreApi command surface.

import init, {
  WasmEngine,
  demo_document_json,
  wing_component_catalog,
} from "../wasm-core/aircraft_wasm.js";
import type {
  AircraftReport,
  CoreApi,
  DocumentMeta,
  MeshDto,
  MeshQuality,
  SourceTraceDto,
  UpdateResultDto,
  WingStations,
} from "./api";
import type { DiagnosticDto } from "./types";

let initPromise: Promise<void> | null = null;

async function ensureInit(): Promise<void> {
  if (!initPromise) {
    initPromise = init().then(() => undefined);
  }
  await initPromise;
}

/** serde_wasm_bindgen renders maps as ES Maps; the UI expects plain
 * objects, so convert recursively. */
function toPlain(value: unknown): unknown {
  if (value instanceof Map) {
    const out: Record<string, unknown> = {};
    for (const [key, entry] of value) {
      out[String(key)] = toPlain(entry);
    }
    return out;
  }
  if (Array.isArray(value)) {
    return value.map(toPlain);
  }
  return value;
}

export class WasmCore implements CoreApi {
  private constructor(private engine: WasmEngine) {}

  static async demoJson(): Promise<string> {
    await ensureInit();
    return demo_document_json();
  }

  /** The wing component catalog, once the wasm module is initialized. */
  static async wingCatalog(): Promise<unknown> {
    await ensureInit();
    return toPlain(wing_component_catalog());
  }

  static async open(json: string): Promise<CoreApi> {
    await ensureInit();
    try {
      return new WasmCore(new WasmEngine(json));
    } catch (error) {
      throw error;
    }
  }

  revision(): number {
    return this.engine.revision();
  }

  documentJson(): string {
    return this.engine.document_json();
  }

  meta(): DocumentMeta {
    return this.engine.document_meta() as DocumentMeta;
  }

  diagnostics(): DiagnosticDto[] {
    return this.engine.diagnostics() as DiagnosticDto[];
  }

  parameters(): Record<string, number> {
    return toPlain(this.engine.parameters()) as Record<string, number>;
  }

  setParameter(id: string, value: number, transaction: number): UpdateResultDto {
    return this.engine.set_parameter(id, value, transaction) as UpdateResultDto;
  }

  addParameter(id: string, value: number, transaction: number): UpdateResultDto {
    return this.engine.add_parameter(id, value, transaction) as UpdateResultDto;
  }

  exportStep(componentIndex: number, fullModel: boolean): string {
    return this.engine.export_step(componentIndex, fullModel) as string;
  }

  setLeTangency(
    componentIndex: number,
    spec: string | null,
    transaction: number,
  ): UpdateResultDto {
    return this.engine.set_le_tangency(componentIndex, spec, transaction) as UpdateResultDto;
  }

  setStationField(
    componentIndex: number,
    stationIndex: number,
    field: string,
    value: unknown,
    transaction: number,
  ): UpdateResultDto {
    return this.engine.set_station_field(
      componentIndex,
      stationIndex,
      field,
      value,
      transaction,
    ) as UpdateResultDto;
  }

  stations(): WingStations[] {
    return this.engine.stations() as WingStations[];
  }

  mesh(quality: MeshQuality, fullModel: boolean): MeshDto {
    const dto = this.engine.mesh(quality, fullModel) as {
      vertices: number[];
      indices: number[];
      [key: string]: unknown;
    };
    // serde hands Vec<f32>/Vec<u32> over as plain arrays; the renderer needs
    // typed arrays.
    return {
      ...(dto as unknown as MeshDto),
      vertices: new Float32Array(dto.vertices),
      indices: new Uint32Array(dto.indices),
    };
  }

  trace(triangleIndex: number, fullModel: boolean): SourceTraceDto {
    return this.engine.trace(triangleIndex, fullModel) as SourceTraceDto;
  }

  report(): AircraftReport {
    return this.engine.report() as AircraftReport;
  }

  exportMesh(
    format: "stl" | "obj" | "glb",
    quality: MeshQuality,
    fullModel: boolean,
  ): Uint8Array {
    return this.engine.export_mesh(format, quality, fullModel);
  }

  snapshot(): CoreApi {
    // The wasm object exposes a deep-copying snapshot method.
    const copy = (this.engine as unknown as { snapshot(): WasmEngine }).snapshot();
    return new WasmCore(copy);
  }

  restore(other: CoreApi): void {
    const source = other as WasmCore;
    this.engine.restore(source.engine);
  }
}
