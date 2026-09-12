import { useEffect, useState } from "react";
import type { CoreApi, StationRow, WingStations } from "../core/api";
import type { FieldName, ValueMode } from "../core/types";
import { typedValue } from "../core/types";
import { useWorkspace } from "../state/store";

interface FieldProps {
  core: CoreApi;
  wingIndex: number;
  station: StationRow;
  field: FieldName;
  label: string;
  kind: string;
  value: number;
  unit: string;
  positiveOnly: boolean;
  parameterPath: string;
}

/** Adaptive range: centered on the current value, never crossing zero when
 * the catalog marks the field positive. Expansion is implicit — the slider
 * re-centers as the value moves. */
function sliderRange(value: number, positiveOnly: boolean): [number, number] {
  const span = Math.max(Math.abs(value) * 0.5, 0.25);
  if (positiveOnly) {
    return [Math.max(value - span, 0.001), value + span];
  }
  return [value - span, value + span];
}

function currentMode(kind: string): ValueMode {
  return kind === "parameter" ? "parameter" : kind === "expression" ? "expression" : "literal";
}

function Field(props: FieldProps) {
  const { core, wingIndex, station, field, label, kind, value, unit, positiveOnly } = props;
  const beginEdit = useWorkspace((s) => s.beginEdit);
  const commit = useWorkspace((s) => s.commit);
  const [mode, setMode] = useState<ValueMode>(currentMode(kind));
  const [draft, setDraft] = useState(String(value));
  const [newParameterId, setNewParameterId] = useState("");
  const [formula, setFormula] = useState("");

  useEffect(() => {
    setMode(currentMode(kind));
    setDraft(String(value));
  }, [kind, value]);

  const stationIndex = useWorkspace((s) =>
    s.stations[s.selectedWing]?.stations.findIndex((row) => row.id === station.id),
  );
  if (stationIndex === undefined || stationIndex < 0) return null;

  const apply = (payload: unknown) => {
    commit((api: CoreApi) =>
      api.setStationField(wingIndex, stationIndex, field, payload, Date.now()),
    );
  };

  const [min, max] = sliderRange(value, positiveOnly);

  return (
    <div className="field" data-field={field}>
      <div className="field-head">
        <span className="field-label">{label}</span>
        <span className="field-unit">{unit}</span>
        <span className="field-modes">
          {(["literal", "parameter", "expression"] as ValueMode[]).map((candidate) => (
            <button
              key={candidate}
              className={mode === candidate ? "mode selected" : "mode"}
              title={`bind ${label} as ${candidate}`}
              onClick={() => setMode(candidate)}
            >
              {candidate === "literal" ? "123" : candidate === "parameter" ? "@" : "fx"}
            </button>
          ))}
        </span>
      </div>

      {mode === "literal" && kind !== "literal" && (
        <button
          className="convert"
          onClick={() => {
            beginEdit();
            apply(typedValue.literal(value));
          }}
        >
          convert to literal {value.toFixed(4)}
        </button>
      )}

      {mode === "literal" && (
        <div className="field-input">
          <input
            type="range"
            min={min}
            max={max}
            step={(max - min) / 200}
            value={value}
            onPointerDown={() => beginEdit()}
            onChange={(event) => {
              beginEdit();
              apply(typedValue.literal(Number(event.target.value)));
            }}
            data-testid={`slider-${field}`}
          />
          <input
            className="number"
            type="number"
            value={draft}
            step="any"
            onChange={(event) => setDraft(event.target.value)}
            onBlur={() => {
              const parsed = Number(draft);
              if (Number.isFinite(parsed) && parsed !== value) {
                beginEdit();
                apply(typedValue.literal(parsed));
              }
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") (event.target as HTMLInputElement).blur();
            }}
            data-testid={`input-${field}`}
          />
        </div>
      )}

      {mode === "parameter" && (
        <div className="field-input">
          <select
            value={kind === "parameter" ? station.bindings[bindKeyOf(field)] ?? "" : ""}
            onChange={(event) => {
              beginEdit();
              apply(typedValue.parameter(event.target.value));
            }}
            data-testid={`select-${field}`}
          >
            <option value="" disabled>
              choose parameter…
            </option>
            {Object.keys(core.parameters()).map((id) => (
              <option key={id} value={id}>
                {id}
              </option>
            ))}
          </select>
          <input
            placeholder="new: wing.myValue"
            value={newParameterId}
            onChange={(event) => setNewParameterId(event.target.value)}
          />
          <button
            disabled={!newParameterId.trim()}
            onClick={() => {
              const id = newParameterId.trim();
              beginEdit();
              const created = core.addParameter(id, value, Date.now());
              if (created.committed) {
                commit((api) =>
                  api.setStationField(
                    wingIndex,
                    stationIndex,
                    field,
                    typedValue.parameter(id),
                    Date.now(),
                  ),
                );
              }
              setNewParameterId("");
            }}
          >
            create &amp; bind
          </button>
          {kind === "parameter" && (
            <span className="bound-value">
              {station.bindings[bindKeyOf(field)]} = {value.toFixed(4)}
            </span>
          )}
        </div>
      )}

      {mode === "expression" && (
        <div className="field-input">
          <input
            className="expression"
            placeholder="= @station.root.position.x + 1"
            value={formula}
            onChange={(event) => setFormula(event.target.value)}
            onBlur={() => {
              if (formula.trim()) {
                beginEdit();
                apply(typedValue.expression(formula.trim()));
              }
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") (event.target as HTMLInputElement).blur();
            }}
            data-testid={`expression-${field}`}
          />
          {kind === "expression" && <span className="bound-value">evaluates to {value.toFixed(4)}</span>}
        </div>
      )}
    </div>
  );
}

/** Map a field name onto its binding slot in the station row. */
function bindKeyOf(field: FieldName): keyof StationRow["bindings"] {
  switch (field) {
    case "position.x":
      return "x";
    case "position.y":
      return "y";
    case "position.z":
      return "z";
    case "twist":
      return "twist";
    default:
      return "chord";
  }
}

export function Inspector() {
  const core = useWorkspace((s) => s.core);
  const stations = useWorkspace((s) => s.stations);
  const wingIndex = useWorkspace((s) => s.selectedWing);
  const stationIndex = useWorkspace((s) => s.selectedStation);
  const meta = useWorkspace((s) => s.meta);

  if (!core || !meta) return <div className="inspector">no document</div>;
  const wing: WingStations | undefined = stations[wingIndex];
  const station = wing?.stations[stationIndex];
  if (!wing || !station) return <div className="inspector">select a station</div>;

  return (
    <div className="inspector" data-testid="inspector">
      <h3>
        {wing.wingId} / station {station.id}
      </h3>
      <div className="station-meta">
        airfoil {station.airfoil} · units {meta.lengthUnit}, {meta.angleUnit}
      </div>
      <Field
        core={core}
        wingIndex={wingIndex}
        station={station}
        field="position.x"
        label="position x"
        kind={station.valueKinds.x}
        value={station.x}
        unit={meta.lengthUnit}
        positiveOnly={false}
        parameterPath={`${wing.wingId}/${station.id}/position.x`}
      />
      <Field
        core={core}
        wingIndex={wingIndex}
        station={station}
        field="position.y"
        label="position y"
        kind={station.valueKinds.y}
        value={station.y}
        unit={meta.lengthUnit}
        positiveOnly={false}
        parameterPath={`${wing.wingId}/${station.id}/position.y`}
      />
      <Field
        core={core}
        wingIndex={wingIndex}
        station={station}
        field="position.z"
        label="position z"
        kind={station.valueKinds.z}
        value={station.z}
        unit={meta.lengthUnit}
        positiveOnly={false}
        parameterPath={`${wing.wingId}/${station.id}/position.z`}
      />
      <Field
        core={core}
        wingIndex={wingIndex}
        station={station}
        field="chord"
        label="chord"
        kind={station.valueKinds.chord}
        value={station.chord}
        unit={meta.lengthUnit}
        positiveOnly={true}
        parameterPath={`${wing.wingId}/${station.id}/chord`}
      />
      <Field
        core={core}
        wingIndex={wingIndex}
        station={station}
        field="twist"
        label="twist"
        kind={station.valueKinds.twist}
        value={station.twist}
        unit={meta.angleUnit}
        positiveOnly={false}
        parameterPath={`${wing.wingId}/${station.id}/twist`}
      />
    </div>
  );
}
