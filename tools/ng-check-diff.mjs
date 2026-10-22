// The differential test of crates/ng_check (PORTING.md, Testing): compares the type-check functions ngcheck generates
// (`ngcheck -p <tsconfig> --keep` leaves them in `<root>/../.ngcheck-<root name>/`) with ngtsc's
// (`tools/ng-check-oracle.mjs`), file by file, function by function.
//
//   node tools/ng-check-diff.mjs <oracle dir> <ngcheck dir> [--show N]
//
// Functions are matched by their component (the type of `this`): ngtsc numbers a file's blocks without the ones it
// inlines into the component's own file (`_tcb_<pos>`, not in the oracle), while ngcheck, as Angular's prototype,
// numbers every component and generates a whole file inline when one of its components needs it. In such a file
// (it holds the component's own source) names are unqualified, so import qualifiers (`i1.`) are dropped from both
// sides. Imports are not compared: ngtsc writes module specifiers relative to absolute paths, ngcheck the sources'
// own specifiers (as Angular's prototype does); both name the same files. Their aliases are numbered by each tool's
// environment (ngtsc's, or the prototype's that ngcheck follows), so a function equal once each side's aliases are
// renumbered by first use counts as "identical but for import numbering". Exit status 1 when a function differs.
import {readdirSync, readFileSync, statSync} from 'node:fs';
import {join, relative} from 'node:path';

import {tcbFunctions as functions} from './ng-check-tcb.mjs';

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

// `i<n>.` aliases renumbered in order of first use
const renumbered = (body) => {
  const order = new Map();
  return body.replace(/\bi(\d+)\./g, (_, n) => {
    if (!order.has(n)) order.set(n, order.size);
    return `i${order.get(n)}.`;
  });
};

let files = 0, same = 0, sameButImports = 0, differ = 0, missing = 0, extra = 0;
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
  // an inline file: the component's source is in it (its classes are declared, not imported)
  const inline = /^(export )?(abstract )?class \w+/m.test(actualText);
  const strip = (body) => (inline ? body.replace(/\bi\d+\./g, '') : body);
  const actual = functions(actualText);
  for (const [name, body] of expected) {
    const ours = actual.get(name);
    if (ours === undefined) {
      missing++;
      if (shown.length < show) shown.push(`MISSING ${rel} ${name}`);
    } else if (strip(ours) === strip(body)) {
      same++;
    } else if (renumbered(strip(ours)) === renumbered(strip(body))) {
      sameButImports++;
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
console.log(
  `\n${files} files: ${same} functions identical, ${sameButImports} identical but for import numbering, ${differ} differ, ` +
    `${missing} missing, ${extra} extra`,
);
process.exit(differ + missing + extra > 0 ? 1 : 0);
