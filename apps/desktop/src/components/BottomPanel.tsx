import { useWorkspace, type BottomTab } from "../state/store";

export function BottomPanel() {
  const tab = useWorkspace((s) => s.bottomTab);
  const setTab = useWorkspace((s) => s.setBottomTab);
  const report = useWorkspace((s) => s.report);
  const trace = useWorkspace((s) => s.trace);
  const diagnostics = useWorkspace((s) => s.diagnostics);
  const meta = useWorkspace((s) => s.meta);

  const tabs: [BottomTab, string, number | null][] = [
    ["report", "report", null],
    ["trace", "trace", null],
    ["diagnostics", "diagnostics", diagnostics.length > 0 ? diagnostics.length : null],
  ];

  return (
    <div className="bottom" data-testid="bottom-panel">
      <div className="bottom-tabs">
        {tabs.map(([id, label, badge]) => (
          <button
            key={id}
            className={tab === id ? "bottom-tab selected" : "bottom-tab"}
            onClick={() => setTab(id)}
            data-testid={`tab-${id}`}
          >
            {label}
            {badge !== null && <span className="badge">{badge}</span>}
          </button>
        ))}
      </div>

      <div className="bottom-body">
        {tab === "report" && (
          <div className="report" data-testid="report">
            {!report && <span>no document loaded</span>}
            {report?.wings.map((wing) => (
              <div key={wing.wingId} className="report-wing">
                <div className="report-title">wing {wing.wingId}</div>
                <table>
                  <tbody>
                    <tr>
                      <td>reference area</td>
                      <td>
                        {wing.planform.referenceArea.half.toFixed(4)} /{" "}
                        {wing.planform.referenceArea.full.toFixed(4)} {meta?.lengthUnit}²
                      </td>
                    </tr>
                    <tr>
                      <td>span</td>
                      <td>
                        {wing.planform.span.half.toFixed(4)} / {wing.planform.span.full.toFixed(4)}{" "}
                        {meta?.lengthUnit}
                      </td>
                    </tr>
                    <tr>
                      <td>MAC</td>
                      <td>
                        {wing.planform.mac.toFixed(4)} {meta?.lengthUnit} at (
                        {wing.planform.macLe[0].toFixed(3)},{" "}
                        {wing.planform.macLe[2].toFixed(3)})
                      </td>
                    </tr>
                    <tr>
                      <td>aspect ratio</td>
                      <td>{wing.planform.aspectRatio.toFixed(4)}</td>
                    </tr>
                    <tr>
                      <td>taper ratio</td>
                      <td>{wing.planform.taperRatio?.toFixed(4) ?? "—"}</td>
                    </tr>
                    {wing.planform.panels.map((panel, index) => (
                      <tr key={index}>
                        <td>panel {index + 1}</td>
                        <td>
                          LE {(panel.leadingEdgeSweep * 180) / Math.PI >= 0 ? "+" : ""}
                          {((panel.leadingEdgeSweep * 180) / Math.PI).toFixed(2)}° · dihedral{" "}
                          {((panel.dihedral * 180) / Math.PI).toFixed(2)}°
                        </td>
                      </tr>
                    ))}
                    {wing.volume && (
                      <tr>
                        <td>volume</td>
                        <td>
                          {wing.volume.volume.half.toFixed(5)} /{" "}
                          {wing.volume.volume.full.toFixed(5)} {meta?.lengthUnit}³
                        </td>
                      </tr>
                    )}
                    <tr>
                      <td>mesh</td>
                      <td>
                        {wing.meshStatistics.vertices} vertices ·{" "}
                        {wing.meshStatistics.triangles} triangles
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
            ))}
          </div>
        )}

        {tab === "trace" && (
          <div className="trace" data-testid="trace">
            {!trace && <span>click a triangle in the viewport to trace its source</span>}
            {trace && (
              <>
                <div className="trace-title">{trace.description}</div>
                <div>
                  wing {trace.wingId}
                  {trace.stations.length > 0 && (
                    <> · stations {trace.stations.join(" → ")}</>
                  )}
                  {trace.mirrored && <> · mirrored half</>}
                  <span className="trace-index"> · triangle #{trace.triangleIndex}</span>
                </div>
              </>
            )}
          </div>
        )}

        {tab === "diagnostics" && (
          <div className="diagnostics" data-testid="diagnostics">
            {diagnostics.length === 0 && <span>no diagnostics</span>}
            {diagnostics.map((diagnostic, index) => (
              <div
                key={index}
                className={
                  diagnostic.severity === "error"
                    ? "diagnostic error"
                    : "diagnostic warning"
                }
              >
                <span className="diagnostic-code">[{diagnostic.code}]</span>{" "}
                {diagnostic.message}
                {diagnostic.subject && <span className="diagnostic-subject"> — {diagnostic.subject}</span>}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
