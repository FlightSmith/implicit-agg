//! Shared UI types.

export interface DiagnosticDto {
  code: string;
  severity: "error" | "warning";
  message: string;
  path?: string;
  subject?: string;
}

export type FieldName =
  | "position.x"
  | "position.y"
  | "position.z"
  | "chord"
  | "twist"
  | "trailingEdge.thickness"
  | "trailingEdge.value";

export type ValueMode = "literal" | "parameter" | "expression";

export interface TypedValueInput {
  literal(value: number): unknown;
  parameter(id: string): unknown;
  expression(formula: string): unknown;
}

export const typedValue = {
  literal: (value: number): unknown => value,
  parameter: (id: string): unknown => ({ $param: id }),
  expression: (formula: string): unknown =>
    formula.startsWith("=") ? formula : `= ${formula}`,
};
