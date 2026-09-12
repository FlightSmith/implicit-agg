//! Catalog-driven control limits for the inspector's numeric editors.
//!
//! The wing component catalog declares, per field: its dimension, its
//! constraint (e.g. strictlyPositive), and an adaptive range policy. This
//! module turns those declarations into concrete slider windows and value
//! clamps, so a wing section simply cannot be dragged or typed into a
//! region that would break the geometry or the view. The engine's semantic
//! validation remains the authority on physical validity; these are the
//! interface limits.

import type { FieldName } from "./types";

interface RangePolicy {
  kind: string;
  initialSpan?: string;
  expand?: string;
}

export interface CatalogField {
  dimension: "length" | "angle" | "ratio";
  constraint?: string;
  editor: string;
  rangePolicy: RangePolicy;
  safetyBounds: { min: number; max: number };
}

export interface WingCatalog {
  fields: Record<string, CatalogField>;
  semanticRules: string[];
}

/** Catalog key per editable field. */
const CATALOG_KEY: Record<FieldName, string> = {
  "position.x": "stations[].position.x",
  "position.y": "stations[].position.y",
  "position.z": "stations[].position.z",
  chord: "stations[].chord",
  twist: "stations[].twist",
  "trailingEdge.thickness": "stations[].trailingEdge.thickness",
  "trailingEdge.value": "stations[].trailingEdge.value",
};

export interface ControlPolicy {
  positiveOnly: boolean;
  hardMin: number;
  hardMax: number;
  /** Fraction-of-current or absolute span used to open the slider window. */
  initialSpan: (value: number) => number;
  expandOnBoundary: boolean;
  dimension: string;
}

const FALLBACK: ControlPolicy = {
  positiveOnly: false,
  hardMin: -100,
  hardMax: 100,
  initialSpan: (value) => Math.max(Math.abs(value) * 0.5, 0.25),
  expandOnBoundary: true,
  dimension: "length",
};

export function policyFor(
  catalog: WingCatalog | null,
  field: FieldName,
): ControlPolicy {
  const entry = catalog?.fields[CATALOG_KEY[field]];
  if (!entry) return FALLBACK;
  const positiveOnly =
    entry.constraint === "strictlyPositive" ||
    entry.rangePolicy.kind === "positiveAroundCurrent" ||
    entry.safetyBounds.min >= 0;
  const dimension = entry.dimension;
  return {
    positiveOnly,
    hardMin: entry.safetyBounds.min,
    hardMax: entry.safetyBounds.max,
    initialSpan: (value) => {
      if (dimension === "angle") return Math.max(Math.abs(value) * 0.5, 5);
      if (dimension === "ratio") return Math.max(Math.abs(value) * 0.5, 0.02);
      return Math.max(Math.abs(value) * 0.5, 0.25);
    },
    expandOnBoundary: entry.rangePolicy.expand === "onBoundary",
    dimension,
  };
}

/** Clamp a typed or dragged value into the catalog's safety bounds. */
export function clampToPolicy(policy: ControlPolicy, value: number): number {
  if (!Number.isFinite(value)) return value;
  return Math.min(Math.max(value, policy.hardMin), policy.hardMax);
}

/** Round to four fraction digits — slider output never carries more. */
export function round4(value: number): number {
  return Math.round(value * 10_000) / 10_000;
}

/**
 * Open a slider window around the current value, intersected with the
 * safety bounds. `expandOnBoundary` fields re-center whenever the value
 * reaches an edge, so the handle can walk the whole legal range.
 */
export function sliderWindow(
  policy: ControlPolicy,
  value: number,
): { min: number; max: number; step: number } {
  const span = policy.initialSpan(value);
  let min = value - span;
  let max = value + span;
  if (policy.positiveOnly) {
    min = Math.max(min, Math.max(policy.hardMin, 1e-4));
  } else {
    min = Math.max(min, policy.hardMin);
  }
  max = Math.min(max, policy.hardMax);
  const step = Math.max(round4((max - min) / 200), 0.0001);
  return { min, max, step };
}

/** True when the value sits on a window edge and the window should grow. */
export function atBoundary(
  window: { min: number; max: number },
  value: number,
  policy: ControlPolicy,
): boolean {
  if (!policy.expandOnBoundary) return false;
  const slack = (window.max - window.min) / 200;
  return value <= window.min + slack || value >= window.max - slack;
}

/** Grow the window in the direction the value pressed against. */
export function expandWindow(
  window: { min: number; max: number },
  value: number,
  policy: ControlPolicy,
): { min: number; max: number; step: number } {
  const span = window.max - window.min;
  let min = window.min;
  let max = window.max;
  if (value >= window.max - span / 200) {
    max = Math.min(round4(value + span), policy.hardMax);
  } else {
    min = Math.max(
      policy.positiveOnly ? Math.max(policy.hardMin, 1e-4) : policy.hardMin,
      round4(value - span),
    );
  }
  const step = Math.max(round4((max - min) / 200), 0.0001);
  return { min, max, step };
}
