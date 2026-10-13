/** Tracks a pointer drag from `e` and reports the offset to the start position. */
export function drag(e: PointerEvent, onMove: (dx: number, ev: PointerEvent) => void, onEnd?: () => void) {
  e.preventDefault();
  const startX = e.clientX;
  const move = (ev: PointerEvent) => onMove(ev.clientX - startX, ev);
  const up = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    document.body.classList.remove("dragging");
    onEnd?.();
  };
  document.body.classList.add("dragging");
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
}

export function loadLayout<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(`layout:${key}`);
    return raw ? (JSON.parse(raw) as T) : fallback;
  } catch {
    return fallback;
  }
}

export function saveLayout(key: string, value: unknown) {
  localStorage.setItem(`layout:${key}`, JSON.stringify(value));
}

export function clamp(v: number, min: number, max: number) {
  return Math.min(max, Math.max(min, v));
}
