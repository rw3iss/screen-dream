import { useState, useRef, useCallback, useEffect } from "preact/hooks";
import { emit } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import styles from "./RegionSelector.module.scss";

interface Point {
  x: number;
  y: number;
}

interface Region {
  x: number;
  y: number;
  width: number;
  height: number;
}

export default function RegionSelector() {
  const [isDragging, setIsDragging] = useState(false);
  const [startPoint, setStartPoint] = useState<Point | null>(null);
  const [currentPoint, setCurrentPoint] = useState<Point | null>(null);
  const overlayRef = useRef<HTMLDivElement>(null);

  const getRegion = (): Region | null => {
    if (!startPoint || !currentPoint) return null;
    const x = Math.min(startPoint.x, currentPoint.x);
    const y = Math.min(startPoint.y, currentPoint.y);
    const width = Math.abs(currentPoint.x - startPoint.x);
    const height = Math.abs(currentPoint.y - startPoint.y);
    if (width < 10 || height < 10) return null;
    return { x, y, width, height };
  };

  const handleMouseDown = useCallback((e: MouseEvent) => {
    e.preventDefault();
    setStartPoint({ x: e.clientX, y: e.clientY });
    setCurrentPoint({ x: e.clientX, y: e.clientY });
    setIsDragging(true);
  }, []);

  const handleMouseMove = useCallback(
    (e: MouseEvent) => {
      if (!isDragging) return;
      setCurrentPoint({ x: e.clientX, y: e.clientY });
    },
    [isDragging],
  );

  const handleMouseUp = useCallback(async () => {
    if (!isDragging) return;
    setIsDragging(false);

    const region = getRegion();
    if (region) {
      // Emit the selected region to the main window.
      await emit("region-selected", region);
    }

    // Hide the overlay window.
    try {
      await invoke("hide_region_selector");
    } catch {
      // Fallback: try closing via window API.
    }

    // Reset state.
    setStartPoint(null);
    setCurrentPoint(null);
  }, [isDragging, startPoint, currentPoint]);

  const handleKeyDown = useCallback(async (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      // Cancel selection.
      await emit("region-cancelled", null);
      try {
        await invoke("hide_region_selector");
      } catch {
        // ignore
      }
      setIsDragging(false);
      setStartPoint(null);
      setCurrentPoint(null);
    }
  }, []);

  useEffect(() => {
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  const region = getRegion();

  return (
    <div
      ref={overlayRef}
      class={styles.regionOverlay}
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
    >
      {/* Instructions */}
      {!isDragging && !region && (
        <div class={styles.instructions}>
          Click and drag to select a region. Press Escape to cancel.
        </div>
      )}

      {/* Selection rectangle */}
      {region && (
        <div
          class={styles.selectionRect}
          style={{
            left: `${region.x}px`,
            top: `${region.y}px`,
            width: `${region.width}px`,
            height: `${region.height}px`,
          }}
        >
          {/* Dimension label */}
          <div class={styles.dimensionLabel}>
            {region.width} x {region.height}
          </div>
        </div>
      )}
    </div>
  );
}
