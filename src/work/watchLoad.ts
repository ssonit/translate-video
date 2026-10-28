// Keys include the content revision so a previous permission check or script
// cannot briefly be presented as belonging to the newly selected result.
export function watchMediaKey(shown: string, revision: number) {
  return JSON.stringify([shown, revision]);
}

export function watchDataKey(result: string, listenFile: string, revision: number) {
  return JSON.stringify([result, listenFile, revision]);
}
