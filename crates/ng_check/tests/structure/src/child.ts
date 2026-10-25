import { Component, input } from '@angular/core';

@Component({ selector: 'x-child, x-kid', standalone: true, templateUrl: './child.html' })
export class Child {
  readonly label = input.required<string>();
}
