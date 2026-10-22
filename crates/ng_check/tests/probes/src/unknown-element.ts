import { Component } from '@angular/core';

// a template error ngc must report
@Component({ selector: 'zz-unknown-element', imports: [], template: `<zz-nope></zz-nope>` })
export class ZzunknownelementProbe {

}
