import { Component } from '@angular/core';
import { ZzChild } from './child';
// a template error ngc must report
@Component({ selector: 'zz-required-input', imports: [ZzChild], template: `<zz-child />` })
export class ZzrequiredinputProbe {

}
