import { Component } from '@angular/core';

// a template error ngc must report
@Component({ selector: 'zz-strict-null', imports: [], template: `{{ maybe.length }}` })
export class ZzstrictnullProbe {
  maybe: string | undefined = undefined;
}
