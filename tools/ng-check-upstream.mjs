// Keeps crates/ng_check in step with Angular (crates/ng_check/PORTING.md).
//
//   node tools/ng-check-upstream.mjs status <commit>   which ported files changed since the pinned commit, with diffs
//   node tools/ng-check-upstream.mjs vendor <commit>   replace vendor/ng-analyze with Angular's at <commit>
//   node tools/ng-check-upstream.mjs pin <commit>      record <commit> as the one the port matches
//
// <commit> is any angular/angular commit, tag or branch. Files come from GitHub; `git` and `diff` must be installed.
import {execFileSync, spawnSync} from 'node:child_process';
import {cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname, join} from 'node:path';
import {fileURLToPath} from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const manifestPath = join(root, 'crates/ng_check/UPSTREAM.json');
const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
const [command, target] = process.argv.slice(2);

async function fetchFile(commit, path) {
  const res = await fetch(`https://raw.githubusercontent.com/angular/angular/${commit}/${path}`);
  if (res.status === 404) return null;
  if (!res.ok) throw new Error(`${path}@${commit}: HTTP ${res.status}`);
  return res.text();
}

// A sparse checkout of `paths` at `commit` in a temporary directory.
function checkout(commit, paths) {
  const dir = mkdtempSync(join(tmpdir(), 'ng-check-upstream-'));
  const git = (...args) => execFileSync('git', args, {cwd: dir, stdio: ['ignore', 'pipe', 'inherit']});
  git('init', '-q');
  git('remote', 'add', 'origin', 'https://github.com/angular/angular.git');
  git('sparse-checkout', 'set', '--no-cone', ...paths.map((p) => `/${p}/`));
  git('fetch', '-q', '--depth', '1', '--filter=blob:none', 'origin', commit);
  git('checkout', '-q', 'FETCH_HEAD');
  return {dir, sha: git('rev-parse', 'HEAD').toString().trim()};
}

if (command === 'status' && target) {
  const pinned = manifest.angular.commit;
  let changed = 0;
  for (const {upstream, rust} of manifest.files) {
    const [before, after] = await Promise.all([fetchFile(pinned, upstream), fetchFile(target, upstream)]);
    if (before === after) continue;
    changed++;
    console.log(`\n=== ${upstream} (${rust})${after === null ? ' REMOVED upstream' : ''}`);
    if (after === null) continue;
    const tmp = mkdtempSync(join(tmpdir(), 'ng-check-diff-'));
    writeFileSync(join(tmp, 'before.ts'), before ?? '');
    writeFileSync(join(tmp, 'after.ts'), after);
    const diff = spawnSync('diff', ['-u', join(tmp, 'before.ts'), join(tmp, 'after.ts')], {encoding: 'utf8'});
    process.stdout.write(diff.stdout);
    rmSync(tmp, {recursive: true, force: true});
  }
  console.log(`\n${changed} of ${manifest.files.length} ported files changed between ${pinned.slice(0, 10)} and ${target}.`);
  console.log('Also compare the TypeScript files these import, and run the differential test (PORTING.md, Testing).');
} else if (command === 'vendor' && target) {
  for (const {upstream, path, omit = []} of manifest.vendored) {
    const {dir, sha} = checkout(target, [upstream]);
    const dest = join(root, path);
    rmSync(dest, {recursive: true, force: true});
    cpSync(join(dir, upstream), dest, {recursive: true});
    for (const name of omit) rmSync(join(dest, name), {force: true});
    rmSync(dir, {recursive: true, force: true});
    console.log(`${path}: ${upstream} at ${sha.slice(0, 10)}`);
  }
} else if (command === 'pin' && target) {
  const {dir, sha} = checkout(target, ['packages/compiler/src/typecheck']);
  rmSync(dir, {recursive: true, force: true});
  manifest.angular.commit = sha;
  writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + '\n');
  console.log(`pinned ${sha}`);
} else {
  console.error('usage: node tools/ng-check-upstream.mjs status|vendor|pin <angular commit>');
  process.exit(2);
}
