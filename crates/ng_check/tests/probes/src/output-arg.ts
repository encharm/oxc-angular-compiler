import { Component } from '@angular/core';
import { ZzChild } from './child';
// a template error ngc must report
@Component({ selector: 'zz-output-arg', imports: [ZzChild], template: `<zz-child label="a" (picked)="takeNumber($event)" />` })
export class ZzoutputargProbe {
  takeNumber(n: number) { return n; }
}
