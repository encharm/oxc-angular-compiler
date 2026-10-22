import { Component } from '@angular/core';
import { ZzChild } from './child';
// a template error ngc must report
@Component({ selector: 'zz-two-way', imports: [ZzChild], template: `<zz-child label="a" [(value)]="num" />` })
export class ZztwowayProbe {
  num = 1;
}
