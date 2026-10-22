import { Component, input, model, output } from '@angular/core';

// the component the probes bind to
@Component({ selector: 'zz-child', template: '' })
export class ZzChild {
  count = input(0);
  label = input.required<string>();
  value = model('');
  picked = output<string>();
}
