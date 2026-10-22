import { Component } from '@angular/core';

// a template error ngc must report
@Component({ selector: 'zz-event-prop', imports: [], template: `<button (click)="use($event.notThere)"></button>` })
export class ZzeventpropProbe {
  use(x: unknown) { return x; }
}
