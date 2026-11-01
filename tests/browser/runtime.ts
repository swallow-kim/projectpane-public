export {};

declare global {
  interface Window {
    startedPaths: string[];
    starts: number;
    cancels: number;
    uiCalls: string[];
  }
}
