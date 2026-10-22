import { Component } from '@angular/core';

// a template error ngc must report
@Component({ selector: 'zz-for-no-track', imports: [], template: `@for (item of items) { {{ item }} }` })
export class ZzfornotrackProbe {
  items = [1];
}
