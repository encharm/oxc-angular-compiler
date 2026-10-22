import { Component } from '@angular/core';

// a template error ngc must report
@Component({ selector: 'zz-for-track', imports: [], template: `@for (item of items; track item.idd) { {{ item.id }} }` })
export class ZzfortrackProbe {
  items = [{ id: 1 }];
}
