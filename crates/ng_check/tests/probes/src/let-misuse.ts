import { Component } from '@angular/core';

// a template error ngc must report
@Component({ selector: 'zz-let-misuse', imports: [], template: `@let n = 5; {{ n.toFixed().nothere }}` })
export class ZzletmisuseProbe {

}
