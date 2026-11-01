export function sanitizeEvidence(report, replacements) {
  return JSON.stringify(report, (_, value) => {
    if (typeof value !== 'string') return value;
    return replacements.reduce((text, [path, label]) => {
      const escaped = JSON.stringify(path).slice(1, -1);
      return text.split(escaped).join(label).split(path).join(label).split(path.replaceAll('\\', '/')).join(label);
    }, value);
  }, 2);
}
