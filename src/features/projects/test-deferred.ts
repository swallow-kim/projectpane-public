export function deferred<T>() {
  let complete: (value: T) => void = () => { throw new Error('Deferred resolver not initialized'); };
  const promise = new Promise<T>(resolve => { complete = resolve; });
  return { promise, resolve: (value: T) => complete(value) };
}
