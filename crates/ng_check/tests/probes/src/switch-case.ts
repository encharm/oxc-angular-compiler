import { Component } from '@angular/core';

// a template error ngc must report
@Component({ selector: 'zz-switch-case', imports: [], template: `@switch (mode) { @case ('z') { z } }` })
export class ZzswitchcaseProbe {
  mode: 'a' | 'b' = 'a';
}
