import { Component } from '@angular/core';
import { NgTemplateOutlet } from '@angular/common';
// a template error ngc must report
@Component({ selector: 'zz-ng-template-ctx', imports: [NgTemplateOutlet], template: `<ng-template #t let-v><b>{{ v.x.y }}</b></ng-template><ng-container *ngTemplateOutlet="t; context: { $implicit: 1 }" />` })
export class ZzngtemplatectxProbe {

}
