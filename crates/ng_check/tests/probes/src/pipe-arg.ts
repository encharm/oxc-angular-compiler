import { Component } from '@angular/core';
import { DatePipe } from '@angular/common';
// a template error ngc must report
@Component({ selector: 'zz-pipe-arg', imports: [DatePipe], template: `{{ today | date: 5 }}` })
export class ZzpipeargProbe {
  today = new Date();
}
