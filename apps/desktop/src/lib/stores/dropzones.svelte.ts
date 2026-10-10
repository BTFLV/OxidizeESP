import { getCurrentWebview } from '@tauri-apps/api/webview';

export interface DropZoneOptions {
  /** Return true if the zone accepts the path. */
  accept: (path: string) => boolean;
  onDrop: (path: string) => void;
}

interface Zone extends DropZoneOptions {
  el: HTMLElement;
}

/**
 * Routes native file drops (delivered by Tauri with absolute paths) to the
 * drop zone under the pointer.
 */
class DropZones {
  #zones = new Set<Zone>();
  hovered = $state<HTMLElement | null>(null);
  dragging = $state(false);
  #initialized = false;

  async init(): Promise<void> {
    if (this.#initialized) return;
    this.#initialized = true;
    await getCurrentWebview().onDragDropEvent((event) => {
      const payload = event.payload;
      if (payload.type === 'leave') {
        this.dragging = false;
        this.hovered = null;
        return;
      }
      const ratio = window.devicePixelRatio || 1;
      const zone = this.#zoneAt(payload.position.x / ratio, payload.position.y / ratio);
      if (payload.type === 'enter' || payload.type === 'over') {
        this.dragging = true;
        this.hovered = zone?.el ?? null;
        return;
      }
      this.dragging = false;
      this.hovered = null;
      const target = zone ?? this.#singleVisibleZone();
      const path = payload.paths.find((p) => target?.accept(p));
      if (target && path) target.onDrop(path);
    });
  }

  #visible(): Zone[] {
    return [...this.#zones].filter((z) => z.el.isConnected && z.el.getClientRects().length > 0);
  }

  #zoneAt(x: number, y: number): Zone | null {
    for (const zone of this.#visible()) {
      const r = zone.el.getBoundingClientRect();
      if (x >= r.left && x <= r.right && y >= r.top && y <= r.bottom) return zone;
    }
    return null;
  }

  #singleVisibleZone(): Zone | null {
    const visible = this.#visible();
    return visible.length === 1 ? visible[0]! : null;
  }

  register(el: HTMLElement, options: DropZoneOptions): () => void {
    const zone: Zone = { el, ...options };
    this.#zones.add(zone);
    return () => this.#zones.delete(zone);
  }
}

export const dropZones = new DropZones();

/** Accept paths with one of the given extensions (case-insensitive). */
export function byExtension(extensions: string[] | null): (path: string) => boolean {
  if (!extensions || extensions.length === 0) return () => true;
  const lower = extensions.map((e) => e.toLowerCase());
  return (path) => {
    const dot = path.lastIndexOf('.');
    return dot >= 0 && lower.includes(path.slice(dot + 1).toLowerCase());
  };
}
