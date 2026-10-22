// The type-check functions of a type-check file, for tools/ng-check-diff.mjs.
//
// Type-check code is not indented, so a function's end is found by brace depth (outside strings, template literals
// and comments), not by a `}` at the start of a line.

// The functions of `text`, by component (`function _tcb1(this: i0.Foo<T>)` is `Foo`), each mapped to its full text
// with its name replaced by `_tcb` (ngtsc and ngcheck number them differently: see ng-check-diff.mjs)
export function tcbFunctions(text) {
  const out = new Map();
  const re = /^function _tcb_?\d+\b(?:<[^(]*>)?\(this: (?:i\d+\.)?(\w+)/gm;
  for (const m of text.matchAll(re)) {
    const end = functionEnd(text, m.index);
    out.set(m[1], text.slice(m.index, end).replace(/^function _tcb_?\d+/, 'function _tcb'));
  }
  return out;
}

// The index just past the `}` that closes the function starting at `start` (its body's opening brace is the first
// `{` met at depth 0 after the parameter list's `)`)
export function functionEnd(text, start) {
  let depth = 0;
  let parens = 0;
  let paramsDone = false;
  let inBody = false;
  // a stack of the template literals being read: the brace depth at which each `${` resumes code
  const templates = [];
  for (let i = start; i < text.length; i++) {
    const c = text[i];
    if (c === '/' && text[i + 1] === '*') {
      i = text.indexOf('*/', i + 2) + 1;
      if (i === 0) return text.length;
    } else if (c === '/' && text[i + 1] === '/') {
      i = text.indexOf('\n', i);
      if (i === -1) return text.length;
    } else if (c === '"' || c === "'") {
      for (i++; i < text.length && text[i] !== c; i++) if (text[i] === '\\') i++;
    } else if (c === '`' || (c === '}' && templates.length > 0 && templates.at(-1) === depth)) {
      // a template literal's text, from its start or from the end of a `${...}`
      if (c === '}') templates.pop();
      for (i++; i < text.length && text[i] !== '`'; i++) {
        if (text[i] === '\\') i++;
        else if (text[i] === '$' && text[i + 1] === '{') {
          templates.push(depth);
          i++;
          break;
        }
      }
    } else if (c === '(') {
      parens++;
    } else if (c === ')') {
      parens--;
      if (parens === 0 && depth === 0) paramsDone = true;
    } else if (c === '{') {
      if (depth === 0 && parens === 0 && paramsDone) inBody = true;
      depth++;
    } else if (c === '}') {
      depth--;
      if (depth === 0 && inBody) return i + 1;
    }
  }
  return text.length;
}
