import { Component } from '@angular/core';

// a template error ngc must report
@Component({ selector: 'zz-unknown-pipe', imports: [], template: `{{ 1 | nopipe }}` })
export class ZzunknownpipeProbe {

}
