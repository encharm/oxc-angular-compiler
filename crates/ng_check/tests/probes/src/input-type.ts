import { Component } from '@angular/core';
import { ZzChild } from './child';
// a template error ngc must report
@Component({ selector: 'zz-input-type', imports: [ZzChild], template: `<zz-child label="a" [count]="'text'" />` })
export class ZzinputtypeProbe {

}
