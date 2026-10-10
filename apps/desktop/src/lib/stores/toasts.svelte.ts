export type ToastTone = 'info' | 'success' | 'warning' | 'danger';

export interface Toast {
  id: number;
  tone: ToastTone;
  title: string;
  description?: string;
}

class ToastState {
  items = $state<Toast[]>([]);
  #next = 1;

  show(tone: ToastTone, title: string, description?: string, timeout = 5000): number {
    const id = this.#next++;
    const toast: Toast = description === undefined ? { id, tone, title } : { id, tone, title, description };
    this.items = [...this.items.slice(-3), toast];
    if (timeout > 0) setTimeout(() => this.dismiss(id), timeout);
    return id;
  }

  dismiss(id: number): void {
    this.items = this.items.filter((t) => t.id !== id);
  }
}

export const toasts = new ToastState();
