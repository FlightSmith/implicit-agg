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

/** A small text sprite for gizmo axis labels. */
function axisLabel(text: string, color: string): THREE.Sprite {
  const canvas = document.createElement("canvas");
  canvas.width = 64;
  canvas.height = 64;
  const context = canvas.getContext("2d")!;
  context.font = "bold 44px sans-serif";
  context.fillStyle = color;
  context.textAlign = "center";
  context.textBaseline = "middle";
  context.fillText(text, 32, 34);
  const sprite = new THREE.Sprite(
    new THREE.SpriteMaterial({ map: new THREE.CanvasTexture(canvas), depthTest: false }),
  );
  sprite.scale.setScalar(0.42);
  return sprite;
}

export function Viewport() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const refs = useRef<SceneRefs | null>(null);
  const mesh = useWorkspace((s) => s.mesh);
  const meshPending = useWorkspace((s) => s.meshPending);
  const meshTier = useWorkspace((s) => s.meshTier);
  const viewMode = useWorkspace((s) => s.viewMode);
  const traceTriangle = useWorkspace((s) => s.trace?.triangleIndex ?? -1);
  const documentId = useWorkspace((s) => s.meta?.id);
  // The camera is fitted once per loaded document and never on edits.
  const hasFit = useRef(false);

  useEffect(() => {
    hasFit.current = false;
  }, [documentId]);

  // One-time scene setup.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    // The logarithmic depth buffer keeps the thin trailing-edge wedge free
    // of z-fighting at close range.
    const renderer = new THREE.WebGLRenderer({
      canvas,
      antialias: true,
      logarithmicDepthBuffer: true,
    });
    renderer.setClearColor(0x11151c);
    renderer.autoClear = false;
    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(45, 1, 0.01, 5000);
    camera.position.set(14, -12, 9);
    camera.up.set(0, 0, 1);

    const controls = new OrbitControls(camera, canvas);
    controls.enableDamping = true;
    controls.rotateSpeed = 0.9;
    controls.zoomToCursor = true;

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

    // Orientation gizmo: miniature aircraft axes rendered in the
    // lower-right corner, oriented with the main camera.
    const gizmoScene = new THREE.Scene();
    const gizmoCamera = new THREE.OrthographicCamera(-1.7, 1.7, 1.7, -1.7, 0.1, 10);
    gizmoCamera.up = camera.up;
    const gizmoAxis = (direction: THREE.Vector3, color: number, label: string) => {
      const geometry = new THREE.BufferGeometry().setFromPoints([
        direction.clone().multiplyScalar(-0.7),
        direction.clone().multiplyScalar(1.0),
      ]);
      gizmoScene.add(new THREE.Line(geometry, new THREE.LineBasicMaterial({ color })));
      const tip = new THREE.Mesh(
        new THREE.SphereGeometry(0.09, 12, 12),
        new THREE.MeshBasicMaterial({ color }),
      );
      tip.position.copy(direction.clone().multiplyScalar(1.0));
      gizmoScene.add(tip);
      const sprite = axisLabel(label, `#${color.toString(16).padStart(6, "0")}`);
      sprite.position.copy(direction.clone().multiplyScalar(1.35));
      gizmoScene.add(sprite);
    };
    gizmoAxis(new THREE.Vector3(1, 0, 0), 0xe06c5a, "X");
    gizmoAxis(new THREE.Vector3(0, 1, 0), 0x7aa25c, "Y");
    gizmoAxis(new THREE.Vector3(0, 0, 1), 0x4fc3f7, "Z");

    const raycaster = new THREE.Raycaster();
    const castAt = (event: MouseEvent) => {
      const current = refs.current;
      if (!current?.mesh) return null;
      const bounds = canvas.getBoundingClientRect();
      const pointer = new THREE.Vector2(
        ((event.clientX - bounds.left) / bounds.width) * 2 - 1,
        -((event.clientY - bounds.top) / bounds.height) * 2 + 1,
      );
      raycaster.setFromCamera(pointer, current.camera);
      return raycaster.intersectObject(current.mesh, false)[0] ?? null;
    };

    const picking = (event: MouseEvent) => {
      const hit = castAt(event);
      const faceIndex = hit?.faceIndex;
      if (faceIndex !== undefined && faceIndex !== null) {
        useWorkspace.getState().selectTrace(faceIndex);
      }
    };
    canvas.addEventListener("click", picking);

    // Double-click re-targets the orbit pivot onto the picked surface
    // point, so close-up inspection rotates around what is on screen.
    const setPivot = (event: MouseEvent) => {
      const hit = castAt(event);
      const current = refs.current;
      if (hit && current) {
        current.controls.target.copy(hit.point);
        current.controls.update();
      }
    };
    canvas.addEventListener("dblclick", setPivot);

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
      renderer.setViewport(0, 0, canvas.clientWidth, canvas.clientHeight);
      renderer.clear();
      renderer.render(scene, camera);
      const gizmoSize = 96;
      const margin = 10;
      renderer.setViewport(
        canvas.clientWidth - gizmoSize - margin,
        margin,
        gizmoSize,
        gizmoSize,
      );
      renderer.clearDepth();
      gizmoCamera.position
        .copy(camera.position)
        .sub(controls.target)
        .normalize()
        .multiplyScalar(4);
      gizmoCamera.lookAt(0, 0, 0);
      renderer.render(gizmoScene, gizmoCamera);
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
    // Debugging affordance: drive the camera from tests and probes.
    (window as unknown as Record<string, unknown>).__viewportRefs = refs;
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
      canvas.removeEventListener("click", picking);
      canvas.removeEventListener("dblclick", setPivot);
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

    // Fit the camera once per document; geometry edits must never move the
    // user's view.
    const box = geometry.boundingBox;
    if (box && !hasFit.current) {
      hasFit.current = true;
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
    }
    if (box) {
      const span = Math.max(
        box.max.x - box.min.x,
        Math.abs(box.max.y - box.min.y),
        box.max.z - box.min.z,
        1,
      );
      const planeScale = span * 2;
      current.symmetryPlane.scale.set(planeScale, 1, planeScale);
    }
  }, [mesh]);

  // Highlight the traced triangle with a translucent filled overlay.
  useEffect(() => {
    const current = refs.current;
    if (!current || !mesh) return;
    const markerName = "trace-marker";
    const previous = current.scene.getObjectByName(markerName) as THREE.Mesh | null;
    if (previous) {
      current.scene.remove(previous);
      previous.geometry.dispose();
    }
    if (traceTriangle < 0 || traceTriangle * 3 + 2 >= mesh.indices.length) return;
    const points: THREE.Vector3[] = [];
    for (let corner = 0; corner < 3; corner++) {
      const index = mesh.indices[traceTriangle * 3 + corner];
      points.push(
        new THREE.Vector3(
          mesh.vertices[index * 3],
          mesh.vertices[index * 3 + 1],
          mesh.vertices[index * 3 + 2],
        ),
      );
    }
    const overlay = new THREE.Mesh(
      new THREE.BufferGeometry().setFromPoints(points),
      new THREE.MeshBasicMaterial({
        color: HIGHLIGHT,
        transparent: true,
        opacity: 0.65,
        side: THREE.DoubleSide,
        depthWrite: false,
        polygonOffset: true,
        polygonOffsetFactor: -2,
        polygonOffsetUnits: -2,
      }),
    );
    overlay.name = markerName;
    overlay.renderOrder = 1;
    current.scene.add(overlay);
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
        <span className="hud-item">grid = 1 m · X aft · Z up</span>
        <span className="hud-item hint">double-click: set orbit pivot</span>
        <span className="hud-item" data-testid="mesh-tier">
          {meshTier === "settled" ? "settled tessellation" : "draft tessellation"}
        </span>
        {meshPending && (
          <span className="hud-item pending" data-testid="mesh-pending">
            recomputing…
          </span>
        )}
      </div>
    </div>
  );
}
