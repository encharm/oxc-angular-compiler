import { Component } from '@angular/core';
import { ZzChild } from './child';
// a template error ngc must report
@Component({ selector: 'zz-attr-to-number', imports: [ZzChild], template: `<zz-child label="a" count="5" />` })
export class ZzattrtonumberProbe {

}
