import { Component } from '@angular/core';

// a template error ngc must report, in an external template
@Component({ selector: 'zz-external', templateUrl: './external.html' })
export class ZzExternalProbe {
  name = 'x';
}
