import { Component } from '@angular/core';

// a template error ngc must report
@Component({ selector: 'zz-host-binding', imports: [], template: ``, host: { '[attr.title]': 'nothere' } })
export class ZzhostbindingProbe {

}
