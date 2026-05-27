import { mount, unmount } from 'svelte';
import ConfirmDialog from './ConfirmDialog.svelte';

export { default as ConfirmDialog } from './ConfirmDialog.svelte';

export interface ConfirmOptions {
  title: string;
  description?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  destructive?: boolean;
}

/**
 * Imperative replacement for `window.confirm`. Mounts a one-shot dialog
 * on `document.body`, returns a promise that resolves to true (confirm)
 * or false (cancel / ESC / backdrop).
 */
export function confirmDialog(opts: ConfirmOptions): Promise<boolean> {
  return new Promise((resolve) => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    let settled = false;
    let component: ReturnType<typeof mount> | null = null;

    const cleanup = (ok: boolean) => {
      if (settled) return;
      settled = true;
      // Allow the dialog close animation to play before unmounting.
      setTimeout(() => {
        if (component) {
          try {
            unmount(component);
          } catch {
            /* no-op */
          }
        }
        target.remove();
      }, 200);
      resolve(ok);
    };

    component = mount(ConfirmDialog, {
      target,
      props: {
        open: true,
        ...opts,
        onResult: cleanup
      }
    });
  });
}
