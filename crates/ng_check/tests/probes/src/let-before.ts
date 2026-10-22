import { Component } from '@angular/core';

// a template error ngc must report
@Component({ selector: 'zz-let-before', imports: [], template: `{{ later }} @let later = 1;` })
export class ZzletbeforeProbe {

}
