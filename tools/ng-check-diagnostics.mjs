// The diagnostics half of the differential test (crates/ng_check/PORTING.md, Testing): compares the errors and
// warnings ngtsc reports (the oracle's diagnostics.json) with ngcheck's output, by file:line:col, code and message.
//   node tools/ng-check-diagnostics.mjs <project dir> <oracle dir> <ngcheck output file>
import {readFileSync} from 'node:fs';
import {join} from 'node:path';
const [dir, oracle, ours] = process.argv.slice(2);
const lineCol = (file, start) => {
  const text = readFileSync(join(dir, file), 'utf8').slice(0, start);
  const lines = text.split('\n');
  return `${lines.length}:${lines.at(-1).length + 1}`;
};
const code = (c) => (c < 0 ? `NG${-c - 990000}` : `TS${c}`);
const expected = JSON.parse(readFileSync(join(oracle, 'diagnostics.json'), 'utf8')).map((d) => ({
  key: `${d.file}:${d.file ? lineCol(d.file, d.start) : ''} ${d.category} ${code(d.code)}`,
  message: d.message.split('\n')[0],
}));
const actual = readFileSync(ours, 'utf8').split('\n').map((l) => l.match(/^(\S+):(\d+):(\d+) - (error|warning) (\w+): (.*)$/)).filter(Boolean)
  .map((m) => ({key: `${m[1]}:${m[2]}:${m[3]} ${m[4]} ${m[5]}`, message: m[6]}));
let same = 0;
const rows = [];
for (const e of expected) {
  const i = actual.findIndex((a) => a.key === e.key);
  if (i < 0) { rows.push(`ONLY ngtsc   ${e.key}  ${e.message.slice(0, 110)}`); continue; }
  const [a] = actual.splice(i, 1);
  if (a.message === e.message) same++;
  else rows.push(`MESSAGE      ${e.key}\n   ngtsc:   ${e.message.slice(0, 150)}\n   ngcheck: ${a.message.slice(0, 150)}`);
}
for (const a of actual) rows.push(`ONLY ngcheck ${a.key}  ${a.message.slice(0, 110)}`);
console.log(rows.join('\n'));
console.log(`\n${expected.length} ngtsc diagnostics: ${same} identical (position, code, message); ${rows.length} rows above`);
