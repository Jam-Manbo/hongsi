import { createHash } from 'node:crypto';

export function stableVersion(tag) {
  const match = /^v((?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*))$/.exec(tag);
  if (!match) throw new Error('Release tag must be vMAJOR.MINOR.PATCH.');
  return match[1];
}

export function releaseVersion(tag, prerelease = false) {
  if (typeof tag !== 'string' || !tag || /[\x00-\x20\x7f]/.test(tag)) throw new Error('Invalid release tag.');
  return prerelease ? tag.replace(/^v(?=\d)/, '') : stableVersion(tag);
}

export function assetStem(tag, prerelease = false) {
  const version = releaseVersion(tag, prerelease);
  if (!prerelease) return `hongsi-${version}`;
  const suffix = /^[a-zA-Z0-9][a-zA-Z0-9._-]{0,99}$/.test(version)
    ? version : createHash('sha256').update(tag).digest('hex').slice(0, 20);
  return `hongsi-beta-${suffix}`;
}

export function urlSegment(value) {
  return encodeURIComponent(value).replace(/[!'()*]/g, char => `%${char.charCodeAt(0).toString(16).toUpperCase()}`);
}

export function androidVersionCode(runNumber) {
  const run = Number(runNumber);
  const code = 100000000 + run;
  if (!Number.isSafeInteger(run) || run < 1 || code > 2100000000) throw new Error('Invalid Android build number.');
  return code;
}
