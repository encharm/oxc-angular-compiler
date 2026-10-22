import { Component } from '@angular/core';

// a template error ngc must report
@Component({ selector: 'zz-if-alias', imports: [], template: `@if (maybe; as m) { {{ m.nothere }} }` })
export class ZzifaliasProbe {
  maybe: { a: number } | null = null;
}
