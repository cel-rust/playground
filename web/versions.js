// The versions of cel-rust the playground offers, each built into a module of
// its own (see build.sh), and listed in versions.json, newest first: which
// module to load, and which version runs what a link asks for.

/** The file of `version`'s module, next to the page. */
export const moduleFile = (version) => `cel-${version}.wasm`;

/** `0.16` for `0.16.1`: one version of each is on offer. */
const lineOf = (version) => version.split(".").slice(0, 2).join(".");

/**
 * Of `versions`, newest first, the one that runs what asks for `wanted`: that
 * version, else the one of its line (`0.16.1` for `0.16.0`), else the newest.
 */
export function pick(versions, wanted) {
  if (typeof wanted !== "string") return versions[0];
  return (
    versions.find((version) => version === wanted) ??
    versions.find((version) => lineOf(version) === lineOf(wanted)) ??
    versions[0]
  );
}
