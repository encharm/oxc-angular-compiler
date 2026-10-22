import { Component } from '@angular/core';

// a template error ngc must report
@Component({ selector: 'zz-ref-var', imports: [], template: `<input #box /> {{ box.valu }}` })
export class ZzrefvarProbe {

}
