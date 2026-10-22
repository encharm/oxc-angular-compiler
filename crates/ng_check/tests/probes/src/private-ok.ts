import { Component } from '@angular/core';

// a template error ngc must report
@Component({ selector: 'zz-private-ok', imports: [], template: `{{ secret }}` })
export class ZzprivateokProbe {
  private secret = 1;
}
