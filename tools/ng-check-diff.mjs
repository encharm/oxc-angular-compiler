// The differential test of crates/ng_check (PORTING.md, Testing): compares the type-check functions ngcheck generates
// (`ngcheck -p <tsconfig> --keep` leaves them in `<root>/../.ngcheck-<root name>/`) with ngtsc's
// (`tools/ng-check-oracle.mjs`), file by file, function by function.
//
//   node tools/ng-check-diff.mjs <oracle dir> <ngcheck dir> [--show N]
//
// Imports are not compared: ngtsc writes module specifiers relative to absolute paths, ngcheck the sources' own
// specifiers (as Angular's prototype does); both name the same files. Exit status 1 when a function differs.
import {readdirSync, readFileSync, statSync} from 'node:fs';
import {join, relative} from 'node:path';

const [oracleDir, oursDir, ...rest] = process.argv.slice(2);
const show = rest[0] === '--show' ? Number(rest[1]) : 5;
if (!oracleDir || !oursDir) {
  console.error('usage: node tools/ng-check-diff.mjs <oracle dir> <ngcheck dir> [--show N]');
  process.exit(2);
}

function* walk(dir) {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) yield* walk(path);
    else if (name.endsWith('.ngtypecheck.ts')) yield path;
  }
}

// The type-check functions of a file: `/*tcbN*/` (ngtsc) or the function name, mapped to the function's text
function functions(text) {
  const out = new Map();
  const re = /^function (_tcb\d+)\b/gm;
  const starts = [...text.matchAll(re)].map((m) => ({name: m[1], index: m.index}));
  for (let i = 0; i < starts.length; i++) {
    const end = i + 1 < starts.length ? starts[i + 1].index : text.length;
    let body = text.slice(starts[i].index, end);
    // up to the function's closing brace at column 0, without what follows (the next id comment, a footer)
    const close = body.search(/^}\s*$/m);
    if (close >= 0) body = body.slice(0, close + 1);
    out.set(starts[i].name, body.trimEnd());
  }
  return out;
}

let files = 0, same = 0, differ = 0, missing = 0, extra = 0;
const shown = [];
for (const oraclePath of walk(oracleDir)) {
  const rel = relative(oracleDir, oraclePath);
  const expected = functions(readFileSync(oraclePath, 'utf8'));
  if (expected.size === 0) continue;
  files++;
  let actualText;
  try {
    actualText = readFileSync(join(oursDir, rel), 'utf8');
  } catch {
    missing += expected.size;
    if (shown.length < show) shown.push(`MISSING ${rel} (${expected.size} functions)`);
    continue;
  }
  const actual = functions(actualText);
  for (const [name, body] of expected) {
    const ours = actual.get(name);
    if (ours === undefined) {
      missing++;
      if (shown.length < show) shown.push(`MISSING ${rel} ${name}`);
    } else if (ours === body) {
      same++;
    } else {
      differ++;
      if (shown.length < show) {
        const a = body.split('\n'), b = ours.split('\n');
        const line = a.findIndex((l, i) => l !== b[i]);
        shown.push(`DIFFERS ${rel} ${name}, line ${line + 1}:\n  ngtsc:   ${a[line]}\n  ngcheck: ${b[line] ?? '(none)'}`);
      }
    }
  }
  for (const name of actual.keys()) if (!expected.has(name)) extra++;
}
for (const s of shown) console.log(s);
console.log(`\n${files} files: ${same} functions identical, ${differ} differ, ${missing} missing, ${extra} extra`);
process.exit(differ + missing + extra > 0 ? 1 : 0);
