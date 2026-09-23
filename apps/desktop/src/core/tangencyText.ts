//! Text notation for a station's leading-edge tangency — the same DSL as the
//! wing-level definition, but scoped to one station:
//!
//!   left    = departure of the LE from this station (tipward)
//!   right   = arrival of the LE at this station (from inboard)
//!   full    = both sides of this station
//!
//! Spec per side: `auto`, or a tangent vector `x,y,z` with an optional
//! `:strength` (0 = straight, 1 = default fullness). Examples:
//!   "left:auto"                  "right:0.8,0,0.1:0.8"
//!   "left:0.55,-0.83,0;full:auto"

import type { StationTangency, StationTangencySide } from "./api";

export interface ParsedStationTangency {
  value: StationTangency;
}

export type ParseError = { error: string };

/** Parse the station tangency text. Station position gates side validity:
 * `right` is meaningless on the root, `left` on the tip. */
export function parseStationTangency(
  text: string,
  station: { isFirst: boolean; isLast: boolean },
): ParsedStationTangency | ParseError {
  const value: StationTangency = { left: null, right: null };
  for (const raw of text.split(";")) {
    const clause = raw.trim();
    if (clause === "") {
      return { error: "empty clause" };
    }
    const colon = clause.indexOf(":");
    if (colon === -1) {
      return { error: `clause "${clause}" must read side:spec (left, right, or full)` };
    }
    const side = clause.slice(0, colon).trim();
    const specText = clause.slice(colon + 1).trim();

    let spec: StationTangencySide;
    if (specText === "auto") {
      spec = { auto: true, direction: null, strength: 1 };
    } else {
      // vector, optionally :strength
      const lastColon = specText.lastIndexOf(":");
      let vectorText = specText;
      let strength = 1;
      if (lastColon !== -1) {
        const maybe = Number(specText.slice(lastColon + 1));
        if (Number.isFinite(maybe)) {
          strength = maybe;
          vectorText = specText.slice(0, lastColon);
        }
      }
      const parts = vectorText.split(",").map((p) => Number(p.trim()));
      if (parts.length !== 3 || parts.some((p) => !Number.isFinite(p))) {
        return { error: `tangent "${specText}" must be x,y,z` };
      }
      spec = {
        auto: false,
        direction: [parts[0], parts[1], parts[2]],
        strength,
      };
    }

    if (side === "full") {
      if (value.left || value.right) {
        return { error: '"full" cannot be combined with other sides' };
      }
      value.left = spec;
      value.right = spec;
    } else if (side === "left" || side === "right") {
      if (side === "left" && station.isLast) {
        return { error: '"left" (departure) is not valid on the tip station' };
      }
      if (side === "right" && station.isFirst) {
        return { error: '"right" (arrival) is not valid on the root station' };
      }
      const slot: "left" | "right" = side;
      if (value[slot]) {
        return { error: `duplicate "${side}" clause` };
      }
      value[slot] = spec;
    } else {
      return {
        error: `unknown side "${side}"; use left, right, or full`,
      };
    }
  }
  if (!value.left && !value.right) {
    return { error: "tangency must name at least one side" };
  }
  return { value };
}

const fn_num = (v: number) => {
  const rounded = Number(v.toFixed(6));
  return String(rounded);
};

function sideText(side: StationTangencySide): string {
  const spec = side.auto
    ? "auto"
    : side.direction
      ? `${fn_num(side.direction[0])},${fn_num(side.direction[1])},${fn_num(side.direction[2])}`
      : "";
  return side.strength !== 1 && !side.auto ? `${spec}:${fn_num(side.strength)}` : spec;
}

/** Serialize a station tangency back to DSL text (empty when unset). */
export function tangencyToText(tangency: StationTangency | null): string {
  if (!tangency) return "";
  const clauses: string[] = [];
  if (tangency.left) clauses.push(`left:${sideText(tangency.left)}`);
  if (tangency.right) clauses.push(`right:${sideText(tangency.right)}`);
  return clauses.join(";");
}
