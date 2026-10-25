import { Directive, input } from '@angular/core';

@Directive({ selector: '[data-hint], [data-hint-value]', standalone: true })
export class Hint {
  readonly name = input<string>(undefined, { alias: 'data-hint' });
  readonly value = input<unknown>(undefined, { alias: 'data-hint-value' });
}
