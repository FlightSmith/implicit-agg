import { useEffect, useMemo, useState } from "react";
import type { CoreApi, StationRow, WingStations } from "../core/api";
import {
  atBoundary,
  clampToPolicy,
  expandWindow,
  policyFor,
  round4,
  sliderWindow,
} from "../core/controlPolicy";
import type { FieldName, ValueMode } from "../core/types";
import { typedValue } from "../core/types";
import { ExpressionInput } from "./ExpressionInput";
import { useWorkspace } from "../state/store";

interface FieldProps {
  core: CoreApi;
  wingIndex: number;
  station: StationRow;
  stations: WingStations[];
  field: FieldName;
  label: string;
  kind: string;
  value: number;
  unit: string;
}

function currentMode(kind: string): ValueMode {
  return kind === "parameter" ? "parameter" : kind === "expression" ? "expression" : "literal";
}

/**
 * One editable numeric field. The slider and the number input are two views
 * of the same edit session: dragging, typing, and the spinner arrows all
 * apply live, values are rounded to four fraction digits, and the catalog's
 * range policy bounds everything the interface can produce. The engine's
 * semantic validation remains the authority on physical validity.
 */
function Field(props: FieldProps) {
  const { core, wingIndex, station, stations, field, label, kind, value, unit } = props;
  const beginEdit = useWorkspace((s) => s.beginEdit);
  const endEdit = useWorkspace((s) => s.endEdit);
  const commit = useWorkspace((s) => s.commit);
  const wingCatalog = useWorkspace((s) => s.wingCatalog);
  const policy = useMemo(() => policyFor(wingCatalog, field), [wingCatalog, field]);

  const [mode, setMode] = useState<ValueMode>(currentMode(kind));
  const [draft, setDraft] = useState(String(value));
  const [range, setRange] = useState(() => sliderWindow(policy, value));
  const [newParameterId, setNewParameterId] = useState("");
  const [formula, setFormula] = useState("");

  // A new station re-opens the adaptive window around its value.
  useEffect(() => {
    setRange(sliderWindow(policy, value));
  }, [policy, station.id]);
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

  /** Literal edits go through the catalog clamp and four-digit rounding. */
  const applyLiteral = (raw: number) => {
    const rounded = round4(clampToPolicy(policy, raw));
    if (!Number.isFinite(rounded) || rounded === value) return;
    apply(typedValue.literal(rounded));
  };

  const onSliderChange = (raw: number) => {
    let next = round4(raw);
    if (atBoundary(range, next, policy)) {
      const grown = expandWindow(range, next, policy);
      setRange(grown);
      next = round4(clampToPolicy(policy, next));
    }
    beginEdit();
    applyLiteral(next);
  };

  const onNumberChange = (text: string) => {
    setDraft(text);
    const parsed = Number(text);
    if (text.trim() !== "" && Number.isFinite(parsed)) {
      beginEdit();
      applyLiteral(parsed);
    }
  };

  const commitDraft = () => {
    endEdit();
    const parsed = Number(draft);
    if (draft.trim() !== "" && Number.isFinite(parsed) && parsed !== value) {
      applyLiteral(parsed);
    }
  };

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
            endEdit();
          }}
        >
          convert to literal {value.toFixed(4)}
        </button>
      )}

      {mode === "literal" && (
        <div className="field-input">
          <input
            type="range"
            min={range.min}
            max={range.max}
            step={range.step}
            value={Math.min(Math.max(value, range.min), range.max)}
            onPointerDown={() => beginEdit()}
            onPointerUp={() => endEdit()}
            onChange={(event) => onSliderChange(Number(event.target.value))}
            data-testid={`slider-${field}`}
          />
          <input
            className="number"
            type="number"
            value={draft}
            step="any"
            onFocus={() => beginEdit()}
            onChange={(event) => onNumberChange(event.target.value)}
            onBlur={commitDraft}
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
              endEdit();
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
              endEdit();
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
          <ExpressionInput
            value={formula}
            parameters={core.parameters()}
            stations={stations}
            testId={`expression-${field}`}
            placeholder="= @station.root.position.x + 1"
            onChange={setFormula}
            onCommit={() => {
              endEdit();
              if (formula.trim()) {
                beginEdit();
                apply(typedValue.expression(formula.trim()));
                endEdit();
              }
            }}
          />
          {kind === "expression" && (
            <span className="bound-value">evaluates to {value.toFixed(4)}</span>
          )}
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

/** Wing-level leading-edge tangency: presets plus the raw DSL. */
function WingTangency({
  wingIndex,
  wing,
}: {
  wingIndex: number;
  wing: WingStations;
}) {
  const beginEdit = useWorkspace((s) => s.beginEdit);
  const endEdit = useWorkspace((s) => s.endEdit);
  const commit = useWorkspace((s) => s.commit);
  const [draft, setDraft] = useState(wing.leTangency ?? "");

  useEffect(() => setDraft(wing.leTangency ?? ""), [wing.leTangency]);

  const apply = (spec: string | null) => {
    beginEdit();
    commit((api) => api.setLeTangency(wingIndex, spec, Date.now()));
    endEdit();
  };

  const commitDraft = () => {
    const text = draft.trim();
    if (text === (wing.leTangency ?? "")) return;
    apply(text === "" ? null : text);
  };

  const presets: [string, string | null][] = [
    ["none", null],
    ["left-auto", "left:auto"],
    ["right-auto", "right:auto"],
    ["full-auto", "full:auto"],
  ];

  return (
    <div className="field" data-testid="tangency">
      <div className="field-head">
        <span className="field-label">wing LE tangency</span>
      </div>
      <div className="presets">
        {presets.map(([label, spec]) => (
          <button
            key={label}
            className={(wing.leTangency ?? null) === spec ? "mode selected" : "mode"}
            onClick={() => apply(spec)}
            data-testid={`tangency-preset-${label}`}
          >
            {label}
          </button>
        ))}
      </div>
      <div className="field-input">
        <input
          className="expression"
          placeholder="left:auto;right:0.8,0,0.1"
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          onBlur={commitDraft}
          onKeyDown={(event) => {
            if (event.key === "Enter") (event.target as HTMLInputElement).blur();
          }}
          data-testid="tangency-input"
        />
      </div>
    </div>
  );
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
      <WingTangency wingIndex={wingIndex} wing={wing} />
      <Field
        core={core}
        wingIndex={wingIndex}
        station={station}
        stations={stations}
        field="position.x"
        label="position x"
        kind={station.valueKinds.x}
        value={station.x}
        unit={meta.lengthUnit}
      />
      <Field
        core={core}
        wingIndex={wingIndex}
        station={station}
        stations={stations}
        field="position.y"
        label="position y"
        kind={station.valueKinds.y}
        value={station.y}
        unit={meta.lengthUnit}
      />
      <Field
        core={core}
        wingIndex={wingIndex}
        station={station}
        stations={stations}
        field="position.z"
        label="position z"
        kind={station.valueKinds.z}
        value={station.z}
        unit={meta.lengthUnit}
      />
      <Field
        core={core}
        wingIndex={wingIndex}
        station={station}
        stations={stations}
        field="chord"
        label="chord"
        kind={station.valueKinds.chord}
        value={station.chord}
        unit={meta.lengthUnit}
      />
      <Field
        core={core}
        wingIndex={wingIndex}
        station={station}
        stations={stations}
        field="twist"
        label="twist"
        kind={station.valueKinds.twist}
        value={station.twist}
        unit={meta.angleUnit}
      />
    </div>
  );
}
