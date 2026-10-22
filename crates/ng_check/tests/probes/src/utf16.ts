import { Component } from '@angular/core';

// a template error ngc must report
@Component({ selector: 'zz-utf16', imports: [], template: `<p>Zażółć gęślą jaźń 😀 {{ nope }}</p>` })
export class Zzutf16Probe {

}
