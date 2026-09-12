import { useWorkspace } from "../state/store";

export function DesignTree() {
  const stations = useWorkspace((s) => s.stations);
  const selectedWing = useWorkspace((s) => s.selectedWing);
  const selectedStation = useWorkspace((s) => s.selectedStation);
  const selectStation = useWorkspace((s) => s.selectStation);
  const meta = useWorkspace((s) => s.meta);

  return (
    <div className="tree" data-testid="design-tree">
      <div className="tree-root">
        {meta ? `${meta.name} (${meta.id})` : "no document"}
      </div>
      {stations.map((wing, wingIndex) => (
        <div key={wing.wingId}>
          <div className="tree-wing">
            wing {wing.wingId}
          </div>
          <ul>
            {wing.stations.map((station, stationIndex) => (
              <li key={station.id}>
                <button
                  className={
                    wingIndex === selectedWing && stationIndex === selectedStation
                      ? "tree-station selected"
                      : "tree-station"
                  }
                  onClick={() => selectStation(wingIndex, stationIndex)}
                >
                  {station.id}
                  <span className="tree-airfoil">{station.airfoil}</span>
                </button>
              </li>
            ))}
          </ul>
        </div>
      ))}
    </div>
  );
}
