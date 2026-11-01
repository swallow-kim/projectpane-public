import { sameRequest, type Identity } from '../../shared/contracts';
import { IpcFault } from '../../shared/fault';

type Task = { readonly key?: string; readonly run: () => Promise<void>; readonly cancel: () => void };

export class ListingQueue {
  private tasks: Task[] = [];
  constructor(private readonly current: () => Identity | undefined, private readonly wake: () => void) {}

  enqueue<T>(identity: Identity, work: () => Promise<T>, key?: string): Promise<T> {
    if (key) {
      this.tasks.filter(task => task.key === key).forEach(task => task.cancel());
      this.tasks = this.tasks.filter(task => task.key !== key);
    }
    if (this.tasks.length >= 16) return Promise.reject(new IpcFault('resource_limit'));
    return new Promise<T>((resolve, reject) => {
      const task = {
        ...(key ? { key } : {}),
        cancel: () => reject(new IpcFault('cancelled')),
        run: async () => {
          try {
            this.assertCurrent(identity);
            const result = await work();
            this.assertCurrent(identity);
            resolve(result);
          } catch (error) { reject(error); }
        },
      };
      this.tasks.push(task);
      this.wake();
    });
  }
  private assertCurrent(identity: Identity) {
    const current = this.current();
    if (!current || !sameRequest(current, identity)) throw new IpcFault('stale_request');
  }
  take(): Task | undefined { return this.tasks.shift(); }
  clear() { this.tasks.forEach(task => task.cancel()); this.tasks = []; }
}
