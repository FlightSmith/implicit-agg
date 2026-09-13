import { useEffect, useMemo, useRef, useState } from "react";
import type { WingStations } from "../core/api";

interface Candidate {
  /** Reference body after the `@`, e.g. `param.wing.rootChord`. */
  ref: string;
  category: "parameter" | "station" | "interface";
}

interface ExpressionInputProps {
  /** Full formula, including the leading `=`. */
  value: string;
  parameters: Record<string, number>;
  stations: WingStations[];
  testId: string;
  placeholder?: string;
  onChange(value: string): void;
  /** Focus left the field with a well-formed formula: commit it. */
  onCommit(): void;
}

/** Characters that terminate a reference (whitespace and operators except
 * `-` and `_`, which belong to identifiers). */
const REF_TERMINATOR = /[\s+*/,()]/;

function buildCandidates(
  parameters: Record<string, number>,
  stations: WingStations[],
): Candidate[] {
  const out: Candidate[] = [];
  for (const id of Object.keys(parameters).sort()) {
    out.push({ ref: `param.${id}`, category: "parameter" });
  }
  for (const wing of stations) {
    for (const station of wing.stations) {
      for (const leaf of ["position.x", "position.y", "position.z", "chord", "twist"]) {
        out.push({ ref: `station.${station.id}.${leaf}`, category: "station" });
      }
    }
    for (const name of wing.interfaces) {
      for (const axis of ["x", "y", "z"]) {
        out.push({
          ref: `component.${wing.wingId}.interface.${name}.origin.${axis}`,
          category: "interface",
        });
      }
    }
  }
  return out;
}

/**
 * Formula editor with `@`-reference autocomplete: pressing `@` opens a list
 * of parameters, stations, and interfaces, filtered as the reference is
 * typed. Arrow keys move, Enter/Tab accept, Escape dismisses; blur commits.
 */
export function ExpressionInput(props: ExpressionInputProps) {
  const { value, parameters, stations, onChange, onCommit, testId, placeholder } = props;
  const inputRef = useRef<HTMLInputElement>(null);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState({ at: 0, text: "" });
  const [active, setActive] = useState(0);

  const suppressRefresh = useRef(false);
  const navigating = useRef(false);
  const all = useMemo(() => buildCandidates(parameters, stations), [parameters, stations]);
  const matches = useMemo(() => {
    const lower = query.text.toLowerCase();
    const filtered = lower
      ? all.filter((candidate) => candidate.ref.toLowerCase().startsWith(lower))
      : all;
    return filtered.slice(0, 50);
  }, [all, query.text]);

  // Re-open the candidate window whenever the caret follows an unterminated
  // `@`.
  const refreshQuery = () => {
    const input = inputRef.current;
    if (!input) return;
    // Arrow-key navigation re-fires this on keyup; keep the active index.
    if (navigating.current) {
      navigating.current = false;
      return;
    }
    // An accepted completion re-runs this effect with the caret right after
    // a complete reference; that must not reopen the popup.
    if (suppressRefresh.current) {
      suppressRefresh.current = false;
      setOpen(false);
      return;
    }
    const caret = input.selectionStart ?? input.value.length;
    const before = input.value.slice(0, caret);
    const at = before.lastIndexOf("@");
    if (at === -1) {
      setOpen(false);
      return;
    }
    const text = before.slice(at + 1);
    if (REF_TERMINATOR.test(text)) {
      setOpen(false);
      return;
    }
    setQuery({ at, text });
    setActive(0);
    setOpen(true);
  };

  useEffect(() => {
    // Value changed externally: re-evaluate the popup context.
    refreshQuery();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [value]);

  const accept = (candidate: Candidate) => {
    const before = value.slice(0, query.at + 1) + candidate.ref;
    const after = value.slice(query.at + 1 + query.text.length);
    const next = before + after;
    suppressRefresh.current = true;
    onChange(next);
    setOpen(false);
    const caret = before.length;
    requestAnimationFrame(() => {
      inputRef.current?.focus();
      inputRef.current?.setSelectionRange(caret, caret);
    });
  };

  const onKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (open && matches.length > 0) {
      if (event.key === "ArrowDown") {
        event.preventDefault();
        navigating.current = true;
        setActive((index) => (index + 1) % matches.length);
        return;
      }
      if (event.key === "ArrowUp") {
        event.preventDefault();
        navigating.current = true;
        setActive((index) => (index - 1 + matches.length) % matches.length);
        return;
      }
      if (event.key === "Enter" || event.key === "Tab") {
        event.preventDefault();
        // Their keyup would otherwise re-run the query and reopen the list.
        navigating.current = true;
        accept(matches[active]);
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        navigating.current = true;
        setOpen(false);
        return;
      }
    }
    if (event.key === "Enter") {
      event.preventDefault();
      onCommit();
      (event.target as HTMLInputElement).blur();
    }
  };

  return (
    <div className="expression-wrap">
      <input
        ref={inputRef}
        className="expression"
        placeholder={placeholder ?? "= @station.root.position.x + 1"}
        value={value}
        onChange={(event) => {
          onChange(event.target.value);
          // React updates the DOM value synchronously for controlled inputs.
          requestAnimationFrame(refreshQuery);
        }}
        onClick={refreshQuery}
        onKeyUp={refreshQuery}
        onKeyDown={onKeyDown}
        onBlur={() => {
          setOpen(false);
          onCommit();
        }}
        data-testid={testId}
      />
      {open && matches.length > 0 && (
        <ul className="autocomplete" data-testid="autocomplete">
          {matches.map((candidate, index) => (
            <li key={candidate.ref}>
              <button
                className={index === active ? "autocomplete-item selected" : "autocomplete-item"}
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => accept(candidate)}
                data-testid="autocomplete-item"
              >
                <span className={`autocomplete-category ${candidate.category}`}>
                  {candidate.category}
                </span>
                <span className="autocomplete-ref">{candidate.ref}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
