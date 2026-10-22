import { Component } from '@angular/core';

// a template error ngc must report
@Component({ selector: 'zz-defer', imports: [], template: `@defer { {{ missing }} } @placeholder { <p>x</p> }` })
export class ZzdeferProbe {

}
