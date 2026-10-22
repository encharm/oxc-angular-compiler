import { Component } from '@angular/core';

// a template error ngc must report
@Component({ selector: 'zz-unknown-property', imports: [], template: `<div [notAProp]="1"></div>` })
export class ZzunknownpropertyProbe {

}
