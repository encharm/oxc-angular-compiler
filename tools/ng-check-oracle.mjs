// The oracle of crates/ng_check's differential test (PORTING.md, Testing): what Angular's own compiler (ngtsc), at the
// commit crates/ng_check/UPSTREAM.json pins, generates for a project's templates.
//
//   node tools/ng-check-oracle.mjs <project tsconfig.json> <out dir>
//
// Writes, under <out dir>, every type-check file ngtsc generated (`<path relative to the tsconfig's directory>`, with `__` for `..`,
// `.ngtypecheck.ts`) and `diagnostics.json` (the errors and warnings ngc reports: file, start, length, code, message).
// The project needs its own node_modules (@angular/core, ...). The compiler is Angular's per-commit build of
// @angular/compiler-cli and @angular/compiler (github.com/angular/*-builds), installed once into
// node_modules/.cache/ng-check-oracle/<commit> with the TypeScript it requires.
import {execFileSync} from 'node:child_process';
import {existsSync, mkdirSync, readFileSync, writeFileSync} from 'node:fs';
import {createRequire} from 'node:module';
import {dirname, join, relative, resolve} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const [tsconfigArg, outArg] = process.argv.slice(2);
if (!tsconfigArg || !outArg) {
  console.error('usage: node tools/ng-check-oracle.mjs <project tsconfig.json> <out dir>');
  process.exit(2);
}
const tsconfig = resolve(tsconfigArg);
const out = resolve(outArg);
const commit = JSON.parse(readFileSync(join(root, 'crates/ng_check/UPSTREAM.json'), 'utf8')).angular.commit;

// The *-builds commit for an angular/angular commit: its message starts with the short hash.
function buildsCommit(repo) {
  const short = commit.slice(0, 7);
  for (let page = 1; page <= 10; page++) {
    const json = execFileSync('gh', ['api', `repos/angular/${repo}/commits?per_page=100&page=${page}`], {encoding: 'utf8'});
    const found = JSON.parse(json).find((c) => c.commit.message.startsWith(short));
    if (found) return found.sha;
  }
  throw new Error(`angular/${repo} has no build of ${short} in its last 1000 commits`);
}

const cache = join(root, 'node_modules/.cache/ng-check-oracle', commit);
if (!existsSync(join(cache, 'node_modules/@angular/compiler-cli'))) {
  mkdirSync(cache, {recursive: true});
  const manifest = {
    private: true,
    dependencies: {
      '@angular/compiler': `github:angular/compiler-builds#${buildsCommit('compiler-builds')}`,
      '@angular/compiler-cli': `github:angular/compiler-cli-builds#${buildsCommit('compiler-cli-builds')}`,
    },
  };
  writeFileSync(join(cache, 'package.json'), JSON.stringify(manifest, null, 2));
  execFileSync('npm', ['install', '--no-audit', '--no-fund', '--registry=https://registry.npmjs.org'], {cwd: cache, stdio: 'inherit'});
  // the TypeScript the compiler requires (its peer dependency)
  const cliManifest = JSON.parse(readFileSync(join(cache, 'node_modules/@angular/compiler-cli/package.json'), 'utf8'));
  const ts = cliManifest.peerDependencies.typescript.split(' ')[0].replace(/^>=/, '~');
  execFileSync('npm', ['install', '--no-audit', '--no-fund', '--registry=https://registry.npmjs.org', `typescript@${ts}`], {cwd: cache, stdio: 'inherit'});
}

const require = createRequire(join(cache, 'package.json'));
const ngtsc = await import(pathToFileURL(require.resolve('@angular/compiler-cli')).href);
const config = ngtsc.readConfiguration(tsconfig);
const options = {...config.options, noEmit: true};
const program = ngtsc.createProgram({
  rootNames: config.rootNames,
  options,
  host: ngtsc.createCompilerHost({options}),
});
await program.loadNgStructureAsync();
const diagnostics = [...program.getTsSemanticDiagnostics(), ...program.getNgStructuralDiagnostics(), ...program.getNgSemanticDiagnostics()];

const base = dirname(tsconfig);
mkdirSync(out, {recursive: true});
let files = 0;
// the program ngtsc type-checks, with the type-check files in it
const checked = program.compiler.getCurrentProgram();
for (const sf of checked.getSourceFiles()) {
  if (!sf.fileName.endsWith('.ngtypecheck.ts') || sf.text.trim().startsWith('export const IS_A_MODULE')) continue;
  // a file outside the tsconfig's directory stays inside <out dir>: each `../` becomes `__/`
  const path = join(out, relative(base, sf.fileName).replace(/\.\.(?=[\\/])/g, '__'));
  mkdirSync(dirname(path), {recursive: true});
  writeFileSync(path, sf.text);
  files++;
}
const message = (m) => (typeof m === 'string' ? m : `${m.messageText}${(m.next ?? []).map((n) => '\n' + message(n)).join('')}`);
const reported = diagnostics
  .filter((d) => d.category === 0 || d.category === 1)
  .map((d) => ({
    file: d.file ? relative(base, d.file.fileName) : null,
    start: d.start ?? null,
    length: d.length ?? null,
    code: d.code,
    category: d.category === 1 ? 'error' : 'warning',
    message: message(d.messageText),
  }));
writeFileSync(join(out, 'diagnostics.json'), JSON.stringify(reported, null, 2) + '\n');
console.log(`${files} type-check files and ${reported.length} diagnostics in ${out}`);
