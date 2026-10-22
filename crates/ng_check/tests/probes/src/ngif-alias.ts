import { Component } from '@angular/core';
import { NgIf } from '@angular/common';
// a template error ngc must report
@Component({ selector: 'zz-ngif-alias', imports: [NgIf], template: `<div *ngIf="maybe as m">{{ m.nothere }}</div>` })
export class ZzngifaliasProbe {
  maybe: { a: number } | null = null;
}
