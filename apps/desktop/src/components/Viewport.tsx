import { useEffect, useRef } from "react";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { useWorkspace } from "../state/store";

interface SceneRefs {
  renderer: THREE.WebGLRenderer;
  scene: THREE.Scene;
  camera: THREE.PerspectiveCamera;
  controls: OrbitControls;
  mesh: THREE.Mesh | null;
  symmetryPlane: THREE.Mesh;
  picking: (event: MouseEvent) => void;
}

const HIGHLIGHT = new THREE.Color("#ff7043");

export function Viewport() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const refs = useRef<SceneRefs | null>(null);
  const mesh = useWorkspace((s) => s.mesh);
  const meshPending = useWorkspace((s) => s.meshPending);
  const viewMode = useWorkspace((s) => s.viewMode);
  const traceTriangle = useWorkspace((s) => s.trace?.triangleIndex ?? -1);

  // One-time scene setup.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const renderer = new THREE.WebGLRenderer({ canvas, antialias: true });
    renderer.setClearColor(0x11151c);
    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(45, 1, 0.05, 5000);
    camera.position.set(14, -12, 9);
    camera.up.set(0, 0, 1);

    const controls = new OrbitControls(camera, canvas);
    controls.enableDamping = true;

    scene.add(new THREE.AmbientLight(0xffffff, 0.55));
    const key = new THREE.DirectionalLight(0xffffff, 1.4);
    key.position.set(6, -8, 12);
    scene.add(key);
    const fill = new THREE.DirectionalLight(0x88aaff, 0.5);
    fill.position.set(-4, 8, -6);
    scene.add(fill);

    // Symmetry plane overlay at y = 0 (the wing-local XZ plane).
    const planeGeometry = new THREE.PlaneGeometry(40, 40);
    planeGeometry.rotateX(Math.PI / 2);
    const symmetryPlane = new THREE.Mesh(
      planeGeometry,
      new THREE.MeshBasicMaterial({
        color: 0x4fc3f7,
        transparent: true,
        opacity: 0.08,
        side: THREE.DoubleSide,
        depthWrite: false,
      }),
    );
    scene.add(symmetryPlane);
    const grid = new THREE.GridHelper(40, 40, 0x2a3b4d, 0x1c2836);
    scene.add(grid);

    const picking = (event: MouseEvent) => {
      const current = refs.current;
      if (!current?.mesh) return;
      const bounds = canvas.getBoundingClientRect();
      const pointer = new THREE.Vector2(
        ((event.clientX - bounds.left) / bounds.width) * 2 - 1,
        -((event.clientY - bounds.top) / bounds.height) * 2 + 1,
      );
      const raycaster = new THREE.Raycaster();
      raycaster.setFromCamera(pointer, current.camera);
      const hits = raycaster.intersectObject(current.mesh, false);
      const faceIndex = hits[0]?.faceIndex;
      if (faceIndex !== undefined && faceIndex !== null) {
        useWorkspace.getState().selectTrace(faceIndex);
      }
    };
    canvas.addEventListener("click", picking);

    const resize = () => {
      const width = canvas.clientWidth || 1;
      const height = canvas.clientHeight || 1;
      renderer.setSize(width, height, false);
      camera.aspect = width / height;
      camera.updateProjectionMatrix();
    };
    resize();
    const observer = new ResizeObserver(resize);
    observer.observe(canvas);

    let frame = 0;
    const loop = () => {
      controls.update();
      renderer.render(scene, camera);
      frame = requestAnimationFrame(loop);
    };
    loop();

    refs.current = {
      renderer,
      scene,
      camera,
      controls,
      mesh: null,
      symmetryPlane,
      picking,
    };
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
      canvas.removeEventListener("click", picking);
      controls.dispose();
      renderer.dispose();
      refs.current = null;
    };
  }, []);

  // Rebuild geometry when a new mesh arrives.
  useEffect(() => {
    const current = refs.current;
    if (!current || !mesh) return;

    if (current.mesh) {
      current.scene.remove(current.mesh);
      current.mesh.geometry.dispose();
      (current.mesh.material as THREE.Material).dispose();
    }

    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute(
      "position",
      new THREE.BufferAttribute(mesh.vertices, 3),
    );
    geometry.setIndex(new THREE.BufferAttribute(mesh.indices, 1));
    geometry.computeVertexNormals();
    geometry.computeBoundingBox();

    const material = new THREE.MeshStandardMaterial({
      color: 0xb0bec5,
      metalness: 0.15,
      roughness: 0.6,
      side: THREE.DoubleSide,
    });
    const sceneMesh = new THREE.Mesh(geometry, material);
    current.scene.add(sceneMesh);
    current.mesh = sceneMesh;

    const box = geometry.boundingBox;
    if (box) {
      const center = box.getCenter(new THREE.Vector3());
      const span = Math.max(
        box.max.x - box.min.x,
        Math.abs(box.max.y - box.min.y),
        box.max.z - box.min.z,
        1,
      );
      // Keep the camera aligned with the aircraft axes: aft +X, starboard -Y
      // on screen, up +Z.
      current.camera.position.set(
        center.x + span * 1.3,
        center.y - span * 1.1,
        center.z + span * 0.8,
      );
      current.controls.target.copy(center);
      current.controls.update();
      const planeScale = span * 2;
      current.symmetryPlane.scale.set(planeScale, 1, planeScale);
    }
  }, [mesh]);

  // Highlight the traced triangle with a small marker.
  useEffect(() => {
    const current = refs.current;
    if (!current || !mesh) return;
    const markerName = "trace-marker";
    const previous = current.scene.getObjectByName(markerName) as THREE.Line | null;
    if (previous) {
      current.scene.remove(previous);
      previous.geometry.dispose();
    }
    if (traceTriangle < 0 || traceTriangle * 3 + 2 >= mesh.indices.length) return;
    const a = mesh.indices[traceTriangle * 3];
    const b = mesh.indices[traceTriangle * 3 + 1];
    const c = mesh.indices[traceTriangle * 3 + 2];
    const points: THREE.Vector3[] = [];
    for (const index of [a, b, c, a]) {
      points.push(
        new THREE.Vector3(
          mesh.vertices[index * 3],
          mesh.vertices[index * 3 + 1],
          mesh.vertices[index * 3 + 2],
        ),
      );
    }
    const line = new THREE.Line(
      new THREE.BufferGeometry().setFromPoints(points),
      new THREE.LineBasicMaterial({ color: HIGHLIGHT }),
    );
    line.name = markerName;
    current.scene.add(line);
  }, [traceTriangle, mesh]);

  return (
    <div className="viewport" data-testid="viewport">
      <canvas ref={canvasRef} className="viewport-canvas" />
      <div className="viewport-hud">
        <span className="hud-item">{viewMode === "full" ? "full model" : "half model"}</span>
        {mesh && (
          <span className="hud-item">
            {mesh.indices.length / 3} triangles · r{mesh.revision}
          </span>
        )}
        {meshPending && (
          <span className="hud-item pending" data-testid="mesh-pending">
            recomputing…
          </span>
        )}
      </div>
    </div>
  );
}
